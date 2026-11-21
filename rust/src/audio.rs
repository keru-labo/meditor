use anyhow::{Context, Result, anyhow, ensure};
use meditor_playback::project::{ClipRange, SECOND, Track};
use sdl2::audio::{AudioQueue, AudioSpecDesired};
use std::{
    fs::File,
    io::{BufReader, Write},
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const RATE: i64 = 48_000;
const CHANNELS: usize = 2;
const BLOCK: usize = 512;
const QUEUE: u32 = (BLOCK * CHANNELS * size_of::<i16>() * 3) as u32;
const POLL: Duration = Duration::from_millis(4);

struct Wave {
    reader: hound::WavReader<BufReader<File>>,
    offset: i64,
    gain: f32,
}

impl Wave {
    fn open(track: Track) -> Result<Self> {
        let reader = hound::WavReader::open(&track.path)
            .with_context(|| format!("音声を開けません ({})", track.name))?;
        let spec = reader.spec();
        ensure!(
            spec.channels == CHANNELS as u16
                && spec.sample_rate == RATE as u32
                && spec.sample_format == hound::SampleFormat::Int
                && spec.bits_per_sample == 16,
            "ステレオ 48 kHz PCM s16le が必要です ({})",
            track.name
        );
        Ok(Self {
            reader,
            offset: track.offset_us * RATE / SECOND,
            gain: track.gain as f32,
        })
    }

    fn mix(&mut self, first: i64, count: usize, output: &mut [f32]) -> Result<()> {
        let source = first - self.offset;
        let begin = source.max(0);
        let end = (source + count as i64).min(i64::from(self.reader.duration()));
        if begin >= end || self.gain == 0.0 {
            return Ok(());
        }
        self.reader.seek(begin as u32)?;
        let skip = (begin - source) as usize * CHANNELS;
        let samples = (end - begin) as usize * CHANNELS;
        for (out, input) in output[skip..skip + samples]
            .iter_mut()
            .zip(self.reader.samples::<i16>())
        {
            *out += f32::from(input?) * self.gain;
        }
        Ok(())
    }
}

fn mix_block(waves: &mut [Wave], first: i64, frames: usize) -> Result<[i16; BLOCK * CHANNELS]> {
    let mut mixed = [0.0; BLOCK * CHANNELS];
    for wave in waves {
        wave.mix(first, frames, &mut mixed)?;
    }
    let mut output = [0; BLOCK * CHANNELS];
    for (out, sample) in output.iter_mut().zip(mixed) {
        *out = sample
            .clamp(f32::from(i16::MIN), f32::from(i16::MAX))
            .round() as i16;
    }
    Ok(output)
}

fn first_sample(position_us: i64) -> i64 {
    (position_us * RATE + SECOND - 1) / SECOND
}

pub(crate) fn write_export_mix(
    tracks: Vec<Track>,
    range: ClipRange,
    mut output: impl Write,
) -> Result<()> {
    let mut waves = tracks
        .into_iter()
        .map(Wave::open)
        .collect::<Result<Vec<_>>>()?;
    let end = first_sample(range.source_end_us);
    let mut first = first_sample(range.source_start_us);
    let mut bytes = [0; BLOCK * CHANNELS * size_of::<i16>()];
    while first < end {
        let frames = (end - first).min(BLOCK as i64) as usize;
        let mixed = mix_block(&mut waves, first, frames)?;
        let byte_count = frames * CHANNELS * size_of::<i16>();
        for (chunk, sample) in bytes[..byte_count]
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip(mixed)
        {
            *chunk = sample.to_le_bytes();
        }
        output.write_all(&bytes[..byte_count])?;
        first += frames as i64;
    }
    Ok(())
}

enum Command {
    Set { epoch: u64, position: Option<i64> },
    Close,
}

#[derive(Default)]
struct Clock {
    epoch: u64,
    position: i64,
    error: Option<String>,
}

pub struct AudioMixer {
    command: mpsc::Sender<Command>,
    clock: Arc<Mutex<Clock>>,
    worker: Option<JoinHandle<()>>,
}

impl AudioMixer {
    pub fn new(tracks: Vec<Track>, end: i64) -> Result<Self> {
        let (command, commands) = mpsc::channel();
        let clock = Arc::new(Mutex::new(Clock::default()));
        let worker_clock = clock.clone();
        let worker = thread::Builder::new()
            .name("meditor-audio".into())
            .spawn(move || {
                if let Err(error) = run(tracks, end, worker_clock.clone(), commands) {
                    worker_clock.lock().unwrap().error = Some(format!("{error:#}"));
                }
            })?;
        Ok(Self {
            command,
            clock,
            worker: Some(worker),
        })
    }

    pub fn start(&self, position: i64) {
        self.set(Some(position));
    }

    pub fn stop(&self) {
        self.set(None);
    }

    fn set(&self, position: Option<i64>) {
        let mut clock = self.clock.lock().unwrap();
        clock.epoch += 1;
        if let Some(position) = position {
            clock.position = position;
        }
        let _ = self.command.send(Command::Set {
            epoch: clock.epoch,
            position,
        });
    }

    pub fn position(&self) -> Result<i64> {
        let clock = self.clock.lock().unwrap();
        if let Some(error) = &clock.error {
            return Err(anyhow!("{error}"));
        }
        Ok(clock.position)
    }
}

impl Drop for AudioMixer {
    fn drop(&mut self) {
        let _ = self.command.send(Command::Close);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct Playback {
    epoch: u64,
    start: i64,
    began: Instant,
    written: i64,
    preroll: bool,
}

fn run(
    tracks: Vec<Track>,
    end: i64,
    clock: Arc<Mutex<Clock>>,
    commands: mpsc::Receiver<Command>,
) -> Result<()> {
    let mut waves = tracks
        .into_iter()
        .map(Wave::open)
        .collect::<Result<Vec<_>>>()?;
    // SDL objects stay on the audio worker. Silent projects use monotonic time.
    let sdl = (!waves.is_empty())
        .then(sdl2::init)
        .transpose()
        .map_err(anyhow::Error::msg)?;
    let audio = sdl
        .as_ref()
        .map(sdl2::Sdl::audio)
        .transpose()
        .map_err(anyhow::Error::msg)?;
    let device: Option<AudioQueue<i16>> = audio
        .as_ref()
        .map(|audio| {
            audio.open_queue(
                None,
                &AudioSpecDesired {
                    freq: Some(RATE as i32),
                    channels: Some(CHANNELS as u8),
                    samples: Some(BLOCK as u16),
                },
            )
        })
        .transpose()
        .map_err(anyhow::Error::msg)?;
    let end_frame = first_sample(end);
    let mut active: Option<Playback> = None;
    loop {
        let command = if active.is_none() {
            match commands.recv() {
                Ok(command) => Some(command),
                Err(_) => break,
            }
        } else {
            match commands.recv_timeout(POLL) {
                Ok(command) => Some(command),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        };
        if let Some(mut command) = command {
            while !matches!(command, Command::Close) {
                let Ok(next) = commands.try_recv() else { break };
                command = next;
            }
            if let Some(device) = &device {
                device.pause();
                device.clear();
            }
            match command {
                Command::Close => break,
                Command::Set { epoch, position } => {
                    active = position.map(|start| Playback {
                        epoch,
                        start,
                        began: Instant::now(),
                        written: first_sample(start),
                        preroll: true,
                    });
                }
            }
        }
        let Some(playback) = &mut active else {
            continue;
        };
        let position = if let Some(device) = &device {
            let queued_bytes = QUEUE.saturating_sub(device.size());
            let block_bytes = (BLOCK * CHANNELS * size_of::<i16>()) as u32;
            for _ in 0..queued_bytes.div_ceil(block_bytes) {
                let count = (end_frame - playback.written).min(BLOCK as i64) as usize;
                if count == 0 {
                    break;
                }
                let output = mix_block(&mut waves, playback.written, count)?;
                let clock = clock.lock().unwrap();
                if clock.epoch != playback.epoch {
                    break;
                }
                device
                    .queue_audio(&output[..count * CHANNELS])
                    .map_err(anyhow::Error::msg)?;
                playback.written += count as i64;
            }
            let clock = clock.lock().unwrap();
            if playback.preroll && clock.epoch == playback.epoch {
                playback.preroll = false;
                device.resume();
            }
            let queued = i64::from(device.size()) / (CHANNELS * size_of::<i16>()) as i64;
            (playback.written - queued) * SECOND / RATE
        } else {
            playback.start + playback.began.elapsed().as_micros() as i64
        };
        let mut clock = clock.lock().unwrap();
        if clock.epoch == playback.epoch {
            clock.position = position.min(end).max(playback.start);
        }
        if position >= end {
            active = None;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_and_speaker_wavs_mix_with_gain_and_offsets() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let tracks = [
            ("game", 100i16, 0, 0.5),
            ("voice-a", 200i16, 21, 0.5),
            ("voice-b", 300i16, -21, 1.0),
        ];
        let mut output = [0.0; 4 * CHANNELS];
        for (name, sample, offset_us, gain) in tracks {
            let path = directory.path().join(format!("{name}.wav"));
            let mut writer = hound::WavWriter::create(&path, spec)?;
            for _ in 0..(3 * CHANNELS) {
                writer.write_sample(sample)?;
            }
            writer.finalize()?;
            Wave::open(Track {
                path,
                name: name.into(),
                offset_us,
                gain,
            })?
            .mix(0, 4, &mut output)?;
        }
        assert_eq!(
            output,
            [350.0, 350.0, 450.0, 450.0, 150.0, 150.0, 100.0, 100.0]
        );
        Ok(())
    }
}
