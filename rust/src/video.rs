use anyhow::{Context, Result, ensure};
use ffmpeg_next as ffmpeg;
use meditor_playback::{SeekRequest, VideoSource, project::MAX_DURATION};
use slint::{Rgb8Pixel, SharedPixelBuffer};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Instant,
};

pub struct VideoFrame {
    pub pts: i64,
    pub pixels: SharedPixelBuffer<Rgb8Pixel>,
}

struct Decoder {
    input: ffmpeg::format::context::Input,
    codec: ffmpeg::decoder::Video,
    stream: usize,
    time_base: ffmpeg::Rational,
    origin: i64,
    duration: i64,
    last_target: Option<i64>,
    draining: bool,
    current: Option<(i64, ffmpeg::frame::Video)>,
    upcoming: Option<(i64, ffmpeg::frame::Video)>,
    shown: Option<Arc<VideoFrame>>,
    scaler: Option<ffmpeg::software::scaling::Context>,
}

impl Decoder {
    fn open(path: &Path) -> Result<Self> {
        ensure!(
            path.to_str().is_some_and(|value| !value.contains('\0')),
            "動画パスを UTF-8 として扱えません"
        );
        let mut options = ffmpeg::Dictionary::new();
        options.set("protocol_whitelist", "file");
        let input = ffmpeg::format::input_with_dictionary(&path, options)
            .with_context(|| format!("動画を開けません ({})", path.display()))?;
        let stream = input
            .streams()
            .best(ffmpeg::media::Type::Video)
            .context("映像ストリームがありません")?;
        let index = stream.index();
        let time_base = stream.time_base();
        let origin = if stream.start_time() == ffmpeg::ffi::AV_NOPTS_VALUE {
            0
        } else {
            stream.start_time()
        };
        let duration = if stream.duration() == ffmpeg::ffi::AV_NOPTS_VALUE {
            input.duration()
        } else {
            ffmpeg::Rescale::rescale(&stream.duration(), time_base, ffmpeg::rescale::TIME_BASE)
        };
        ensure!(
            (1..=MAX_DURATION).contains(&duration),
            "長さが既知の24時間以内の動画が必要です"
        );
        let mut context = ffmpeg::codec::context::Context::from_parameters(stream.parameters())?;
        let mut threading =
            ffmpeg::codec::threading::Config::kind(ffmpeg::codec::threading::Type::Slice);
        threading.count = 2;
        context.set_threading(threading);
        let codec = context.decoder().video()?;
        Ok(Self {
            input,
            codec,
            stream: index,
            time_base,
            origin,
            duration,
            last_target: None,
            draining: false,
            current: None,
            upcoming: None,
            shown: None,
            scaler: None,
        })
    }

    fn next_frame(
        &mut self,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Option<(i64, ffmpeg::frame::Video)>> {
        let mut frame = ffmpeg::frame::Video::empty();
        while !cancelled() {
            match self.codec.receive_frame(&mut frame) {
                Ok(()) => {
                    if let Some(timestamp) = frame.timestamp() {
                        let timestamp = timestamp.saturating_sub(self.origin);
                        return Ok(Some((
                            ffmpeg::Rescale::rescale(
                                &timestamp,
                                self.time_base,
                                ffmpeg::rescale::TIME_BASE,
                            ),
                            frame,
                        )));
                    }
                    continue;
                }
                Err(ffmpeg::Error::Eof) => return Ok(None),
                Err(ffmpeg::Error::Other { errno }) if errno == ffmpeg::error::EAGAIN => {}
                Err(error) => return Err(error.into()),
            }
            if self.draining {
                return Ok(None);
            }
            let mut packet = ffmpeg::Packet::empty();
            loop {
                if cancelled() {
                    return Ok(None);
                }
                match packet.read(&mut self.input) {
                    Ok(()) if packet.stream() == self.stream => {
                        self.codec.send_packet(&packet)?;
                        break;
                    }
                    Ok(()) => {}
                    Err(ffmpeg::Error::Eof) => {
                        self.draining = true;
                        self.codec.send_eof()?;
                        break;
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        }
        Ok(None)
    }

    fn origin_us(&self) -> i64 {
        ffmpeg::Rescale::rescale(&self.origin, self.time_base, ffmpeg::rescale::TIME_BASE)
    }

    fn at(
        &mut self,
        target: i64,
        end: i64,
        cancelled: impl Fn() -> bool,
    ) -> Result<Option<Arc<VideoFrame>>> {
        let end = end.min(self.duration);
        let target = target.clamp(0, end - 1);
        if self
            .last_target
            .is_none_or(|last| target < last || target - last > 300_000)
        {
            let timestamp = self.origin_us().saturating_add(target);
            self.input.seek(timestamp, ..timestamp)?;
            self.codec.flush();
            self.current = None;
            self.upcoming = None;
            self.shown = None;
            self.draining = false;
        }
        self.last_target = Some(target);
        loop {
            if self.upcoming.is_none() {
                self.upcoming = self.next_frame(&cancelled)?;
            }
            if cancelled() {
                self.last_target = None;
                return Ok(None);
            }
            let Some((pts, _)) = self.upcoming.as_ref() else {
                break;
            };
            if *pts >= end || (*pts > target && self.current.is_some()) {
                break;
            }
            self.current = self.upcoming.take();
            self.shown = None;
        }
        let Some((pts, frame)) = &self.current else {
            return Ok(None);
        };
        if self.shown.is_none() {
            use ffmpeg::{
                format::Pixel,
                software::scaling::{Context, Flags},
            };
            let scale = (960.0 / f64::from(frame.width()))
                .min(540.0 / f64::from(frame.height()))
                .min(1.0);
            let width = ((f64::from(frame.width()) * scale) as u32).max(1);
            let height = ((f64::from(frame.height()) * scale) as u32).max(1);
            if self.scaler.as_ref().is_none_or(|scaler| {
                let input = scaler.input();
                input.format != frame.format()
                    || input.width != frame.width()
                    || input.height != frame.height()
            }) {
                self.scaler = Some(Context::get(
                    frame.format(),
                    frame.width(),
                    frame.height(),
                    Pixel::RGB24,
                    width,
                    height,
                    Flags::FAST_BILINEAR,
                )?);
            }
            let mut output = ffmpeg::frame::Video::empty();
            self.scaler.as_mut().unwrap().run(frame, &mut output)?;
            let stride = output.stride(0);
            let row = width as usize * 3;
            let mut pixels = SharedPixelBuffer::<Rgb8Pixel>::new(width, height);
            for (destination, source) in pixels
                .make_mut_bytes()
                .chunks_exact_mut(row)
                .zip(output.data(0).chunks_exact(stride))
            {
                destination.copy_from_slice(&source[..row]);
            }
            self.shown = Some(Arc::new(VideoFrame { pts: *pts, pixels }));
        }
        Ok(self.shown.clone())
    }
}

#[derive(Clone, Copy)]
pub struct VideoRequest {
    pub seek: Option<SeekRequest>,
    pub target: i64,
    pub source_end: i64,
    pub issued: Instant,
}

impl VideoRequest {
    pub fn source(self) -> VideoSource {
        self.seek.map_or(VideoSource::Original, SeekRequest::source)
    }
}

pub struct Preview {
    pub request: VideoRequest,
    pub frame: Result<Arc<VideoFrame>>,
}

#[derive(Default)]
struct Queue {
    pending: Option<VideoRequest>,
    ready: Option<Preview>,
    closed: bool,
}

#[derive(Default)]
struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
    revision: AtomicU64,
}

pub struct VideoWorker {
    pub duration: i64,
    pub source_stream: usize,
    pub source_origin_us: i64,
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl VideoWorker {
    pub fn open(source: PathBuf, proxy: Option<PathBuf>) -> Result<Self> {
        let shared = Arc::new(Shared::default());
        let worker_shared = shared.clone();
        let (opened, response) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("meditor-video".into())
            .spawn(move || {
                let mut original = match Decoder::open(&source) {
                    Ok(decoder) => decoder,
                    Err(error) => {
                        let _ = opened.send(Err(error));
                        return;
                    }
                };
                if opened
                    .send(Ok((
                        original.duration,
                        original.stream,
                        original.origin_us(),
                    )))
                    .is_err()
                {
                    return;
                }
                let mut proxy_decoder = None;
                let shared = worker_shared;
                loop {
                    let mut queue = shared.queue.lock().unwrap();
                    while queue.pending.is_none() && !queue.closed {
                        queue = shared.wake.wait(queue).unwrap();
                    }
                    if queue.closed {
                        break;
                    }
                    let request = queue.pending.take().unwrap();
                    let revision = shared.revision.load(Ordering::Relaxed);
                    drop(queue);
                    let cancelled = || shared.revision.load(Ordering::Acquire) != revision;
                    let frame = (|| {
                        let decoder = match request.source() {
                            VideoSource::Original => &mut original,
                            VideoSource::Proxy => {
                                if proxy_decoder.is_none() {
                                    let decoder = Decoder::open(
                                        proxy.as_deref().context("プロキシがありません")?,
                                    )?;
                                    ensure!(
                                        decoder.duration.abs_diff(original.duration) <= 100_000,
                                        "元動画とプロキシの長さが異なります"
                                    );
                                    proxy_decoder = Some(decoder);
                                }
                                proxy_decoder.as_mut().unwrap()
                            }
                        };
                        decoder
                            .at(request.target, request.source_end, cancelled)?
                            .context("指定位置の映像がありません")
                    })();
                    let mut queue = shared.queue.lock().unwrap();
                    if !cancelled() {
                        queue.ready = Some(Preview { request, frame });
                    }
                }
            })?;
        match response.recv() {
            Ok(Ok((duration, source_stream, source_origin_us))) => Ok(Self {
                duration,
                source_stream,
                source_origin_us,
                shared,
                worker: Some(worker),
            }),
            result => {
                let _ = worker.join();
                Err(result?.unwrap_err())
            }
        }
    }

    pub fn submit(&self, request: VideoRequest) {
        let mut queue = self.shared.queue.lock().unwrap();
        self.shared.revision.fetch_add(1, Ordering::AcqRel);
        queue.ready = None;
        queue.pending = Some(request);
        self.shared.wake.notify_one();
    }

    pub fn take(&self) -> Option<Preview> {
        self.shared.queue.lock().unwrap().ready.take()
    }
}

impl Drop for VideoWorker {
    fn drop(&mut self) {
        let mut queue = self.shared.queue.lock().unwrap();
        queue.closed = true;
        self.shared.revision.fetch_add(1, Ordering::AcqRel);
        self.shared.wake.notify_one();
        drop(queue);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{process::Command, time::Duration};

    #[test]
    fn seeks_to_the_frame_covering_the_clip_position_and_respects_the_exclusive_end() -> Result<()>
    {
        ffmpeg::init()?;
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("映像.mp4");
        let status = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=96x64:rate=30:duration=2",
                "-threads",
                "1",
                "-c:v",
                "libx264",
                "-g",
                "30",
            ])
            .arg(&path)
            .status()
            .context("FFmpeg CLI が PATH に必要です")?;
        ensure!(status.success(), "動画フィクスチャの作成に失敗しました");

        let worker = VideoWorker::open(path, None)?;
        let source_end = 1_013_000;
        for (target, expected_pts) in [(1_013_000, 1_000_000), (415_000, 400_000)] {
            let issued = Instant::now();
            worker.submit(VideoRequest {
                seek: None,
                target,
                source_end,
                issued,
            });
            let frame = loop {
                if let Some(result) = worker.take() {
                    assert_eq!(result.request.target, target);
                    break result.frame?;
                }
                ensure!(
                    issued.elapsed() < Duration::from_secs(5),
                    "デコードのタイムアウト"
                );
                thread::sleep(Duration::from_millis(2));
            };
            assert_eq!(frame.pts, expected_pts);
            assert!(frame.pts < source_end);
        }
        Ok(())
    }
}
