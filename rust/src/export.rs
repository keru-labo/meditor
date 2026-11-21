use crate::audio::write_export_mix;
use anyhow::{Context, Result, ensure};
use meditor_playback::project::{Caption, ClipRange, Project, SECOND, Track};
use std::{
    fs::{self, File},
    path::PathBuf,
    process::{Command, Stdio},
};

const CAPTION_HEADER: &str = "[Script Info]\n\
    ScriptType: v4.00+\nPlayResX: 640\nPlayResY: 360\nWrapStyle: 0\n\
    [V4+ Styles]\n\
    Format: Name,Fontname,Fontsize,PrimaryColour,SecondaryColour,OutlineColour,BackColour,Bold,Italic,Underline,StrikeOut,ScaleX,ScaleY,Spacing,Angle,BorderStyle,Outline,Shadow,Alignment,MarginL,MarginR,MarginV,Encoding\n\
    Style: Default,sans-serif,24,&H00FFFFFF,&H00FFFFFF,&H00101010,&H00000000,0,0,0,0,100,100,0,0,1,2,0,2,16,16,20,1\n\
    [Events]\n\
    Format: Layer,Start,End,Style,Name,MarginL,MarginR,MarginV,Effect,Text\n";

pub(crate) struct ExportSnapshot {
    video: PathBuf,
    stream: usize,
    seek_us: i64,
    range: ClipRange,
    audio: Vec<Track>,
    captions: Vec<Caption>,
}

impl ExportSnapshot {
    pub(crate) fn capture(project: &Project, duration: i64, stream: usize, origin_us: i64) -> Self {
        let range = project.playback_range(duration);
        let captions = project
            .captions
            .iter()
            .filter(|caption| caption.start_us < range.duration())
            .cloned()
            .collect();
        Self {
            video: project.video.clone(),
            stream,
            seek_us: origin_us.saturating_add(range.source_start_us),
            range,
            audio: project.audio.clone(),
            captions,
        }
    }

    pub(crate) fn write(self, mut destination: PathBuf) -> Result<PathBuf> {
        if destination.extension().is_none() {
            destination.set_extension("mp4");
        }
        ensure!(
            destination
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("mp4")),
            "書き出し先には .mp4 を指定してください"
        );
        let destination = std::path::absolute(destination)?;
        ensure!(!destination.exists(), "既存のファイルは上書きしません");
        let parent = destination
            .parent()
            .context("書き出し先のフォルダーがありません")?;
        let workspace =
            tempfile::tempdir_in(parent).context("書き出し先に一時フォルダーを作成できません")?;
        let output = tempfile::NamedTempFile::new_in(workspace.path())?.into_temp_path();
        let video = std::path::absolute(&self.video)?;
        let log = workspace.path().join("ffmpeg.log");
        let mut command = Command::new(ffmpeg_on_path()?);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW keeps the console hidden while the GUI is active.
            command.creation_flags(0x0800_0000);
        }
        command
            .current_dir(workspace.path())
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-xerror",
                "-nostdin",
                "-y",
            ])
            .args([
                "-abort_on",
                "empty_output_stream",
                "-protocol_whitelist",
                "file",
            ])
            .args(["-seek_timestamp", "1", "-ss", &seconds(self.seek_us), "-i"])
            .arg(video)
            .stdout(Stdio::null())
            .stderr(File::create(&log)?);
        if self.audio.is_empty() {
            command.stdin(Stdio::null()).arg("-an");
        } else {
            command
                .stdin(Stdio::piped())
                .args(["-f", "s16le", "-ar", "48000", "-ac", "2", "-i", "pipe:0"])
                .args(["-map", "1:a:0", "-c:a", "aac", "-b:a", "192k"]);
        }
        let mut video_filter = String::from("pad=ceil(iw/2)*2:ceil(ih/2)*2");
        if !self.captions.is_empty() {
            fs::write(
                workspace.path().join("captions.ass"),
                caption_script(&self.captions, self.range.duration()),
            )?;
            // Subtitle path stays constant, including when the project path contains filter syntax.
            video_filter.push_str(",ass=filename=captions.ass");
        }
        command
            .args([
                "-map",
                &format!("0:{}", self.stream),
                "-vf",
                &video_filter,
                "-t",
            ])
            .arg(seconds(self.range.duration()))
            .args([
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                "20",
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
                "-f",
                "mp4",
            ])
            .arg(output.file_name().unwrap());

        let mut child = command
            .spawn()
            .context("書き出しには PATH 上の FFmpeg が必要です")?;
        let mix = if let Some(stdin) = child.stdin.take() {
            write_export_mix(self.audio, self.range, stdin)
        } else {
            Ok(())
        };
        // Always reap FFmpeg and remove the staged output on failure.
        let status = child.wait().context("FFmpeg の書き出し結果を待てません")?;
        if let Err(error) = mix {
            let closed = error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::BrokenPipe);
            if !closed || status.success() {
                return Err(error.context("書き出し用音声を読み込めません"));
            }
        }
        if !status.success() {
            let reason = fs::read_to_string(&log).unwrap_or_default();
            let first_error = reason.lines().find(|line| !line.is_empty()).unwrap_or("");
            anyhow::bail!("FFmpeg で書き出せませんでした ({status}) {first_error}");
        }
        output
            .persist_noclobber(&destination)
            .context("書き出し先に新しいファイルを保存できません")?;
        Ok(destination)
    }
}

fn ffmpeg_on_path() -> Result<PathBuf> {
    let program = format!("ffmpeg{}", std::env::consts::EXE_SUFFIX);
    std::env::var_os("PATH")
        .and_then(|path| {
            std::env::split_paths(&path)
                .filter(|dir| dir.is_absolute())
                .map(|dir| dir.join(&program))
                .find(|entry| entry.is_file())
        })
        .context("書き出しには PATH 上の FFmpeg が必要です")
}

fn seconds(micros: i64) -> String {
    let magnitude = micros.unsigned_abs();
    let sign = if micros < 0 { "-" } else { "" };
    format!(
        "{sign}{}.{:06}",
        magnitude / SECOND as u64,
        magnitude % SECOND as u64
    )
}

fn ass_time(micros: i64) -> String {
    let units = (micros + 9_999) / 10_000;
    let seconds = units / 100;
    format!(
        "{}:{:02}:{:02}.{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60,
        units % 100
    )
}

fn caption_script(captions: &[Caption], duration: i64) -> String {
    use std::fmt::Write as _;
    let mut script = CAPTION_HEADER.to_owned();
    for caption in captions {
        let end = caption.end_us.min(duration);
        if caption.start_us >= end {
            continue;
        }
        let text = if caption.speaker.is_empty() {
            caption.text.clone()
        } else {
            format!("{}: {}", caption.speaker, caption.text)
        };
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let mut encoded = String::with_capacity(text.len());
        for ch in text.chars() {
            match ch {
                '\n' => encoded.push_str("\\N"),
                '\\' => encoded.push_str("\\\u{2060}"),
                '{' | '}' => {
                    encoded.push('\\');
                    encoded.push(ch);
                }
                '\t' => encoded.push(' '),
                ch if ch.is_control() => {}
                ch => encoded.push(ch),
            }
        }
        let _ = writeln!(
            script,
            "Dialogue: 0,{},{},Default,,0,0,0,,{encoded}",
            ass_time(caption.start_us),
            ass_time(end),
        );
    }
    script
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, path::Path};

    fn ffmpeg(args: &[&str], path: &Path) -> Result<Vec<u8>> {
        let output = Command::new(ffmpeg_on_path()?)
            .args(["-hide_banner", "-loglevel", "error", "-nostdin"])
            .args(args)
            .arg(path)
            .output()?;
        ensure!(
            output.status.success(),
            "FFmpeg fixture failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(output.stdout)
    }

    #[test]
    fn caption_script_keeps_plain_text_and_clips_to_timeline_end() {
        let cue = Caption {
            id: "one".into(),
            start_us: 100_000,
            end_us: 3_000_000,
            speaker: "話者A".into(),
            text: "first\r\n{\\pos(1,1)}".into(),
        };
        let script = caption_script(&[cue], 2_000_000);
        assert!(script.contains("0:00:00.10,0:00:02.00"));
        assert!(script.contains("話者A: first\\N\\{\\\u{2060}pos(1,1)\\}"));
        assert_eq!(script.matches("Dialogue:").count(), 1);
    }

    #[test]
    fn mp4_contains_burned_in_captions_and_the_offset_wav_mix() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("元動画 (1).mp4");
        ffmpeg(
            &[
                "-f",
                "lavfi",
                "-i",
                "color=black:size=192x108:rate=10:duration=3",
                "-vf",
                "drawbox=x=0:y=0:w=48:h=32:color=white:t=fill:enable='lt(t,0.5)+gte(t,2.5)'",
                "-c:v",
                "libx264",
            ],
            &source,
        )?;
        let wav = directory.path().join("話者.wav");
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&wav, spec)?;
        for frame in 0..96_000 {
            let sample = ((frame as f32 * std::f32::consts::TAU / 100.0).sin() * 7_000.0) as i16;
            writer.write_sample(sample)?;
            writer.write_sample(sample)?;
        }
        writer.finalize()?;
        let mut project = Project::new(
            source,
            vec![Track {
                path: wav,
                name: "話者A".into(),
                offset_us: 900_000,
                gain: 0.5,
            }],
        )?;
        project.set_clip(
            ClipRange {
                source_start_us: 500_000,
                source_end_us: 2_500_000,
            },
            3_000_000,
        )?;
        project.captions.push(Caption {
            id: "one".into(),
            start_us: 200_000,
            end_us: 900_000,
            speaker: "話者A".into(),
            text: "{\\an9}実況\nSecond line".into(),
        });
        let snapshot = ExportSnapshot::capture(&project, 3_000_000, 0, 0);
        project.captions.clear();
        project.audio.clear();
        let output = snapshot.write(directory.path().join("書き出し '日本語'.mp4"))?;
        let frame = |time| -> Result<Vec<u8>> {
            let pixels = ffmpeg(
                &[
                    "-ss",
                    time,
                    "-sn",
                    "-i",
                    output.to_str().unwrap(),
                    "-frames:v",
                    "1",
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "gray",
                    "-threads",
                    "1",
                ],
                Path::new("pipe:1"),
            )?;
            assert_eq!(pixels.len(), 192 * 108);
            Ok(pixels)
        };
        let burned = frame("0.4")?;
        let (top, bottom) = burned.split_at(192 * 54);
        // Caption text must appear without an enabled subtitle track or injected top alignment.
        assert!(top.iter().all(|&pixel| pixel < 10));
        assert!(bottom.iter().any(|&pixel| pixel > 80));
        assert!(frame("0.1")?.iter().all(|&pixel| pixel < 10));
        assert!(frame("0.9")?.iter().all(|&pixel| pixel < 10));
        assert!(frame("1.9")?.iter().all(|&pixel| pixel < 10));
        let audio = ffmpeg(
            &[
                "-i",
                output.to_str().unwrap(),
                "-map",
                "0:a:0",
                "-f",
                "s16le",
            ],
            Path::new("pipe:1"),
        )?;
        assert!(audio.len() >= 2 * 48_000 * 2 * 2);
        let peak = |start: usize, end: usize| {
            audio[start * 4..end * 4]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&pair| i16::from_le_bytes(pair).unsigned_abs())
                .max()
                .unwrap_or_default()
        };
        assert!(peak(4_800, 9_600) < 100, "sound before the offset");
        assert!(peak(38_400, 43_200) > 1_500, "expected shifted sound");

        let existing = directory.path().join("keep.mp4");
        let mut file = File::create(&existing)?;
        file.write_all(b"keep this file")?;
        drop(file);
        let error = ExportSnapshot::capture(&project, 3_000_000, 0, 0)
            .write(existing.clone())
            .unwrap_err();
        assert!(error.to_string().contains("上書き"));
        assert_eq!(fs::read(existing)?, b"keep this file");
        Ok(())
    }
}
