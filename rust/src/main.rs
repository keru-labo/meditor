#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod audio;
mod video;

use anyhow::{Context, Result, ensure};
use audio::AudioMixer;
use meditor_playback::{
    SeekController, SeekMode, VideoSource,
    history::{Edit, EditHistory, EditTarget},
    project::{ClipRange, Project, SECOND, Track, import_srt, parse_seconds},
};
use serde_json::{Value, json};
use slint::{ComponentHandle, Model, Timer, TimerMode, VecModel};
use std::{
    cell::RefCell,
    ffi::OsString,
    fs::File,
    path::PathBuf,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use video::{Preview, VideoFrame, VideoRequest, VideoWorker};

slint::include_modules!();

const TICK: Duration = Duration::from_millis(8);
const SCRUB_INTERVAL: Duration = Duration::from_millis(32);
const PLAY_INTERVAL: Duration = Duration::from_millis(16);
const SLIDER_RANGE: i64 = 1_000_000;

#[derive(Clone, Copy)]
enum Leave {
    Open,
    New,
    Quit,
}

#[derive(Default)]
struct Metrics {
    enabled: bool,
    events: Vec<Value>,
    dropped: usize,
    previous_tick: Option<Instant>,
}

impl Metrics {
    fn record(&mut self, event: impl FnOnce() -> Value) {
        if self.enabled {
            if self.events.len() < 100_000 {
                self.events.push(event());
            } else {
                self.dropped += 1;
            }
        }
    }
}

struct Loaded {
    project: Project,
    path: PathBuf,
    video: VideoWorker,
    audio: Option<AudioMixer>,
    dirty: bool,
    history: EditHistory,
}

impl Loaded {
    fn range(&self) -> ClipRange {
        self.project.playback_range(self.video.duration)
    }

    fn open(project: Project, path: PathBuf) -> Result<Self> {
        let proxy = (!project.proxy.as_os_str().is_empty()).then(|| project.proxy.clone());
        let video = VideoWorker::open(project.video.clone(), proxy)?;
        if let Some(clip) = project.clip {
            ensure!(
                clip.valid_for(video.duration),
                "素材区間が動画の長さを超えています"
            );
        }
        Ok(Self {
            project,
            path,
            video,
            audio: None,
            dirty: false,
            history: EditHistory::default(),
        })
    }
}

#[derive(Default)]
struct Editor {
    loaded: Option<Loaded>,
    captions: Rc<VecModel<CaptionRow>>,
    selected: Option<usize>,
    seek: SeekController,
    position: i64,
    resume_pending: bool,
    playing: bool,
    playback_request_in_progress: bool,
    scrubbing: bool,
    resume_after_scrub: bool,
    scrub_pending: Option<VideoRequest>,
    last_scrub_send: Option<Instant>,
    last_play_send: Option<Instant>,
    last_frame: Option<Arc<VideoFrame>>,
    leaving: Option<Leave>,
    metrics: Metrics,
    smoke: Option<Smoke>,
}

struct Smoke {
    screenshot: Option<PathBuf>,
    deadline: Instant,
    phase: SmokePhase,
}

#[derive(Clone, Copy, Debug)]
enum SmokePhase {
    FirstFrame,
    Playing,
    Proxy,
    Resuming,
    Capture(Instant),
}

impl Editor {
    fn install(&mut self, ui: &EditorWindow, loaded: Loaded) {
        self.stop(ui);
        self.scrubbing = false;
        self.scrub_pending = None;
        ui.set_frame(slint::Image::default());
        self.last_frame = None;
        self.metrics = Metrics::default();
        ui.set_measuring(false);
        ui.set_has_project(true);
        ui.set_filename(
            loaded
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .as_ref()
                .into(),
        );
        let has_proxy = !loaded.project.proxy.as_os_str().is_empty();
        ui.set_has_proxy(has_proxy);
        ui.set_use_proxy(has_proxy);
        let status = format!("音声 {} トラック", loaded.project.audio.len());
        self.loaded = Some(loaded);
        self.refresh_history_controls(ui);
        self.refresh_captions(ui);
        self.show_clip(ui);
        self.seek_original(ui, 0, false);
        ui.set_status(status.into());
    }

    fn request_leave(&mut self, ui: &EditorWindow, leaving: Leave) -> Result<()> {
        if self.loaded.as_ref().is_some_and(|loaded| loaded.dirty) {
            self.leaving = Some(leaving);
            ui.set_confirm_discard(true);
            return Ok(());
        }
        self.leave(ui, leaving)
    }

    fn leave(&mut self, ui: &EditorWindow, leaving: Leave) -> Result<()> {
        match leaving {
            Leave::Quit => slint::quit_event_loop().map_err(anyhow::Error::msg),
            Leave::Open => {
                if let Some(path) = json_dialog("プロジェクトを開く").pick_file() {
                    self.install(ui, Loaded::open(Project::load(&path)?, path)?);
                }
                Ok(())
            }
            Leave::New => {
                let Some(video) = video_dialog("元動画を選択").pick_file() else {
                    return Ok(());
                };
                let wavs = rfd::FileDialog::new()
                    .set_title("ゲーム音・話者の WAV を選択（キャンセルで音声なし）")
                    .add_filter("PCM WAV", &["wav"])
                    .pick_files()
                    .unwrap_or_default();
                let gain = 1.0 / wavs.len().max(1) as f64;
                let tracks = wavs
                    .into_iter()
                    .map(|path| Track {
                        name: path
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                        path,
                        offset_us: 0,
                        gain,
                    })
                    .collect();
                let Some(path) = json_dialog("プロジェクトを保存")
                    .set_file_name("project.json")
                    .save_file()
                else {
                    return Ok(());
                };
                let mut loaded = Loaded::open(Project::new(video, tracks)?, path)?;
                loaded.project.save(&loaded.path)?;
                self.install(ui, loaded);
                Ok(())
            }
        }
    }

    fn save(&mut self, ui: &EditorWindow) -> Result<()> {
        if let Some(loaded) = &mut self.loaded {
            loaded.project.save(&loaded.path)?;
            loaded.dirty = false;
            ui.set_status("プロジェクトを保存しました".into());
        }
        Ok(())
    }

    fn choose_proxy(&mut self, ui: &EditorWindow) -> Result<()> {
        let Some(loaded) = &self.loaded else {
            return Ok(());
        };
        let Some(path) = video_dialog("同じタイムラインのプロキシを選択").pick_file()
        else {
            return Ok(());
        };
        let worker = VideoWorker::open(loaded.project.video.clone(), Some(path.clone()))?;
        if let Some(clip) = loaded.project.clip {
            ensure!(
                clip.valid_for(worker.duration),
                "素材区間が動画の長さを超えています"
            );
        }
        let resume = self.playing || self.resume_pending;
        let loaded = self.loaded.as_mut().unwrap();
        loaded.project.proxy = path;
        loaded.video = worker;
        loaded.dirty = true;
        ui.set_has_proxy(true);
        ui.set_use_proxy(true);
        self.seek_original(ui, self.position, resume);
        Ok(())
    }

    fn import(&mut self, ui: &EditorWindow) -> Result<()> {
        let Some(loaded) = &mut self.loaded else {
            return Ok(());
        };
        if !loaded.project.captions.is_empty()
            && rfd::MessageDialog::new()
                .set_title("字幕の置き換え")
                .set_description("現在の字幕を置き換えますか？")
                .set_buttons(rfd::MessageButtons::YesNo)
                .show()
                != rfd::MessageDialogResult::Yes
        {
            return Ok(());
        }
        if let Some(path) = rfd::FileDialog::new()
            .set_title("UTF-8 SRT を選択")
            .add_filter("SubRip", &["srt"])
            .pick_file()
        {
            loaded.project.captions = import_srt(&path)?;
            loaded.dirty = true;
            // The replacement changes the identities and positions of subtitle rows.
            loaded.history = EditHistory::default();
            self.refresh_history_controls(ui);
            self.refresh_captions(ui);
            ui.set_status("字幕を読み込みました".into());
        }
        Ok(())
    }

    fn refresh_captions(&mut self, ui: &EditorWindow) {
        let loaded = self.loaded.as_ref().unwrap();
        self.captions = Rc::new(VecModel::from_iter(
            loaded.project.captions.iter().map(caption_row),
        ));
        ui.set_captions(self.captions.clone().into());
        self.selected = None;
        ui.set_selected(-1);
        ui.set_speaker_text("".into());
        ui.set_caption_text("".into());
        ui.set_selection_label("字幕を選択して編集".into());
    }

    fn select(&mut self, ui: &EditorWindow, index: i32) -> Option<i64> {
        let row = usize::try_from(index).ok()?;
        let caption = self.loaded.as_ref()?.project.captions.get(row)?;
        self.selected = Some(row);
        ui.set_selected(index);
        ui.set_speaker_text(caption.speaker.as_str().into());
        ui.set_caption_text(caption.text.as_str().into());
        ui.set_selection_label(
            format!(
                "{} – {}",
                timestamp(caption.start_us),
                timestamp(caption.end_us)
            )
            .into(),
        );
        Some(caption.start_us)
    }

    fn edit_caption(&mut self, ui: &EditorWindow) -> Result<()> {
        let began = Instant::now();
        let Some(loaded) = &mut self.loaded else {
            return Ok(());
        };
        let Some(index) = self.selected else {
            return Ok(());
        };
        let caption = &loaded.project.captions[index];
        let speaker = ui.get_speaker_text();
        let text = ui.get_caption_text();
        if caption.speaker == speaker.as_str() && caption.text == text.as_str() {
            return Ok(());
        }
        let edit = Edit::Caption {
            row: index,
            id: caption.id.clone(),
            speaker: speaker.to_string(),
            text: text.to_string(),
        };
        if loaded
            .history
            .apply(&mut loaded.project, loaded.video.duration, edit)?
            .is_some()
        {
            self.captions
                .set_row_data(index, caption_row(&loaded.project.captions[index]));
            loaded.dirty = true;
            self.refresh_history_controls(ui);
            self.metrics.record(|| json!({"event":"caption_edit_dispatch", "us": began.elapsed().as_micros() as u64}));
        }
        Ok(())
    }

    fn show_clip(&self, ui: &EditorWindow) {
        let range = self.loaded.as_ref().unwrap().range();
        ui.set_clip_start(seconds_text(range.source_start_us).into());
        ui.set_clip_end(seconds_text(range.source_end_us).into());
    }

    fn edit_clip(&mut self, ui: &EditorWindow) -> Result<()> {
        let Some(loaded) = self.loaded.as_mut() else {
            return Ok(());
        };
        let before = loaded.range();
        let range = ClipRange {
            source_start_us: parse_seconds(ui.get_clip_start().as_str())
                .context("開始時刻を確認してください")?,
            source_end_us: parse_seconds(ui.get_clip_end().as_str())
                .context("終了時刻を確認してください")?,
        };
        if before != range
            && loaded
                .history
                .apply(
                    &mut loaded.project,
                    loaded.video.duration,
                    Edit::Clip(Some(range)),
                )?
                .is_some()
        {
            loaded.dirty = true;
            self.refresh_clip(ui, before.source_start_us);
            self.refresh_history_controls(ui);
            ui.set_status("クリップの区間を更新しました".into());
        }
        Ok(())
    }

    fn refresh_clip(&mut self, ui: &EditorWindow, previous_start: i64) {
        let loaded = self.loaded.as_mut().unwrap();
        let start = loaded.range().source_start_us;
        loaded.audio = None;
        self.show_clip(ui);
        self.seek_original(ui, previous_start + self.position - start, false);
    }

    fn refresh_history_controls(&self, ui: &EditorWindow) {
        let history = self.loaded.as_ref().map(|loaded| &loaded.history);
        ui.set_can_undo(history.is_some_and(EditHistory::can_undo));
        ui.set_can_redo(history.is_some_and(EditHistory::can_redo));
    }

    fn replay_history(&mut self, ui: &EditorWindow, redo: bool) -> Result<()> {
        let Some(loaded) = &mut self.loaded else {
            return Ok(());
        };
        let before = loaded.range();
        let duration = loaded.video.duration;
        let target = if redo {
            loaded.history.redo(&mut loaded.project, duration)?
        } else {
            loaded.history.undo(&mut loaded.project, duration)?
        };
        let Some(target) = target else {
            return Ok(());
        };
        loaded.dirty = true;
        match target {
            EditTarget::Caption(index) => {
                self.captions
                    .set_row_data(index, caption_row(&loaded.project.captions[index]));
                if self.selected == Some(index) {
                    self.select(ui, index as i32);
                }
            }
            EditTarget::Clip => self.refresh_clip(ui, before.source_start_us),
        }
        self.refresh_history_controls(ui);
        ui.set_status(
            if redo {
                "やり直しました"
            } else {
                "元に戻しました"
            }
            .into(),
        );
        Ok(())
    }

    fn stop(&mut self, ui: &EditorWindow) {
        if let Some(audio) = self
            .loaded
            .as_ref()
            .and_then(|loaded| loaded.audio.as_ref())
        {
            audio.stop();
        }
        self.seek.invalidate();
        self.playing = false;
        self.resume_pending = false;
        self.playback_request_in_progress = false;
        ui.set_playing(false);
    }

    fn show_position(&mut self, ui: &EditorWindow, position: i64) {
        let duration = self.loaded.as_ref().unwrap().range().duration();
        self.position = position.clamp(0, duration);
        ui.set_playhead((self.position * SLIDER_RANGE / duration) as i32);
        ui.set_clock_label(
            format!("{}  /  {}", timestamp(self.position), timestamp(duration)).into(),
        );
    }

    fn video_request(&self, seek: Option<meditor_playback::SeekRequest>) -> VideoRequest {
        let range = self.loaded.as_ref().unwrap().range();
        VideoRequest {
            seek,
            target: range.source_start_us + self.position,
            source_end: range.source_end_us,
            issued: Instant::now(),
        }
    }

    fn seek_original(&mut self, ui: &EditorWindow, position: i64, resume: bool) {
        if self.loaded.is_none() {
            return;
        }
        self.stop(ui);
        self.scrubbing = false;
        self.scrub_pending = None;
        self.show_position(ui, position);
        self.resume_pending =
            resume && self.position < self.loaded.as_ref().unwrap().range().duration();
        ui.set_playing(self.resume_pending);
        let seek = self.seek.request(
            self.position as u64,
            SeekMode::Settle {
                resume_playback: self.resume_pending,
            },
        );
        let request = self.video_request(Some(seek));
        self.loaded.as_ref().unwrap().video.submit(request);
    }

    fn scrub(&mut self, ui: &EditorWindow, value: i32) {
        let Some(loaded) = &self.loaded else { return };
        let duration = loaded.range().duration();
        if !self.scrubbing {
            self.resume_after_scrub = self.playing || self.resume_pending;
            self.stop(ui);
            self.scrubbing = true;
            self.last_scrub_send = None;
        }
        self.show_position(ui, duration * i64::from(value) / SLIDER_RANGE);
        let proxy_available = ui.get_use_proxy() && ui.get_has_proxy();
        let request = self
            .seek
            .request(self.position as u64, SeekMode::Scrub { proxy_available });
        self.scrub_pending = Some(self.video_request(Some(request)));
        self.metrics.record(
            || json!({"event":"scrub_input", "target_us":self.position, "proxy":proxy_available}),
        );
    }

    fn finish_scrub(&mut self, ui: &EditorWindow, value: i32) {
        if self.scrubbing {
            let duration = self.loaded.as_ref().unwrap().range().duration();
            self.seek_original(
                ui,
                duration * i64::from(value) / SLIDER_RANGE,
                self.resume_after_scrub,
            );
        }
    }

    fn toggle_play(&mut self, ui: &EditorWindow) {
        let Some(loaded) = self.loaded.as_ref() else {
            return;
        };
        if self.scrubbing {
            return;
        }
        if self.playing || self.resume_pending {
            self.seek_original(ui, self.position, false);
        } else {
            let position = if self.position == loaded.range().duration() {
                0
            } else {
                self.position
            };
            self.seek_original(ui, position, true);
        }
    }

    fn tick(&mut self, ui: &EditorWindow) -> Result<()> {
        if self.loaded.is_none() {
            return Ok(());
        }
        let now = Instant::now();
        if self.smoke.is_some() && self.tick_smoke(ui, now)? {
            return Ok(());
        }
        if self.metrics.enabled
            && let Some(previous) = self.metrics.previous_tick.replace(now)
        {
            self.metrics.record(|| json!({"event":"gui_timer_interval", "us":now.duration_since(previous).as_micros() as u64}));
        }
        let loaded = self.loaded.as_ref().unwrap();
        if self
            .last_scrub_send
            .is_none_or(|last| now - last >= SCRUB_INTERVAL)
            && let Some(request) = self.scrub_pending.take()
        {
            self.last_scrub_send = Some(now);
            loaded.video.submit(request);
        }
        if let Some(preview) = loaded.video.take() {
            self.receive(ui, preview)?;
        }
        if self.playing {
            let loaded = self.loaded.as_ref().unwrap();
            let range = loaded.range();
            let position = loaded.audio.as_ref().unwrap().position()? - range.source_start_us;
            self.show_position(ui, position);
            if position >= range.duration() {
                self.stop(ui);
            } else if !self.playback_request_in_progress
                && self
                    .last_play_send
                    .is_none_or(|last| now - last >= PLAY_INTERVAL)
            {
                self.last_play_send = Some(now);
                self.playback_request_in_progress = true;
                self.loaded
                    .as_ref()
                    .unwrap()
                    .video
                    .submit(self.video_request(None));
            }
        }
        Ok(())
    }

    fn receive(&mut self, ui: &EditorWindow, preview: Preview) -> Result<()> {
        if let Some(seek) = preview.request.seek {
            if !self.seek.accept_frame(seek) {
                return Ok(());
            }
        } else {
            self.playback_request_in_progress = false;
            if !self.playing {
                return Ok(());
            }
        }

        let frame = match preview.frame {
            Ok(frame) => frame,
            Err(error) if preview.request.source() == VideoSource::Proxy => {
                ui.set_status(format!("{error:#}。元動画を使用します").into());
                ui.set_use_proxy(false);
                let seek = self.seek.request(
                    self.position as u64,
                    SeekMode::Scrub {
                        proxy_available: false,
                    },
                );
                self.scrub_pending = Some(self.video_request(Some(seek)));
                return Ok(());
            }
            Err(error) => return Err(error),
        };

        if !self
            .last_frame
            .as_ref()
            .is_some_and(|shown| Arc::ptr_eq(&frame, shown))
        {
            ui.set_frame(slint::Image::from_rgb8(frame.pixels.clone()));
            self.last_frame = Some(frame.clone());
        }
        let loaded = self.loaded.as_mut().unwrap();
        let range = loaded.range();
        let pts = frame.pts - range.source_start_us;
        if self.scrubbing {
            self.metrics.record(|| {
                json!({
                    "event":"scrub_frame_submit",
                    "us":preview.request.issued.elapsed().as_micros() as u64,
                    "target_us":preview.request.target - range.source_start_us,
                    "video_pts_us":pts,
                    "proxy":preview.request.source() == VideoSource::Proxy,
                })
            });
        }
        if preview
            .request
            .seek
            .is_some_and(|request| request.resumes_playback())
        {
            if loaded.audio.is_none() {
                loaded.audio = Some(AudioMixer::new(
                    loaded.project.audio.clone(),
                    range.source_end_us,
                )?);
            }
            let audio = loaded.audio.as_ref().unwrap();
            audio.start(range.source_start_us + self.position);
            self.resume_pending = false;
            self.playing = true;
            self.last_play_send = None;
            self.metrics.record(|| {
                json!({
                    "event":"resume_frame_submit", "video_pts_us":pts,
                    "audio_requested_position_us":self.position,
                })
            });
        }
        Ok(())
    }

    fn tick_smoke(&mut self, ui: &EditorWindow, now: Instant) -> Result<bool> {
        let smoke = self.smoke.as_ref().unwrap();
        ensure!(
            now < smoke.deadline,
            "プレビューのスモークテストがタイムアウトしました ({:?}, playing={}, resume={}, position_us={})",
            smoke.phase,
            self.playing,
            self.resume_pending,
            self.position
        );
        let loaded = self.loaded.as_ref().unwrap();
        let scrub_target = loaded.range().duration() / 4;
        let phase = match smoke.phase {
            SmokePhase::FirstFrame if self.last_frame.is_some() => {
                ensure!(
                    loaded.project.captions.len() == 10_000 && loaded.project.audio.len() == 3,
                    "合成プロジェクトの字幕10,000件と音声3本が必要です"
                );
                let frame = self.last_frame.as_ref().unwrap();
                ensure!(
                    frame.pixels.width() == 960
                        && frame.pixels.as_bytes().iter().any(|&channel| channel > 80),
                    "元動画の初回フレームを確認できません"
                );
                self.toggle_play(ui);
                SmokePhase::Playing
            }
            SmokePhase::Playing if self.playing && self.position >= 50_000 => {
                self.scrub(ui, 750_000);
                self.scrub(ui, 250_000);
                SmokePhase::Proxy
            }
            SmokePhase::Proxy
                if self
                    .last_frame
                    .as_ref()
                    .is_some_and(|frame| frame.pixels.width() == 480) =>
            {
                let frame = self.last_frame.as_ref().unwrap();
                ensure!(
                    frame
                        .pts
                        .abs_diff(scrub_target + loaded.range().source_start_us)
                        <= 35_000,
                    "最新のプロキシシーク位置と映像が一致しません"
                );
                self.finish_scrub(ui, 250_000);
                SmokePhase::Resuming
            }
            SmokePhase::Resuming if self.playing && self.position >= scrub_target + 50_000 => {
                ensure!(
                    self.last_frame
                        .as_ref()
                        .is_some_and(|frame| frame.pixels.width() == 960),
                    "元動画と音声で再開できません"
                );
                self.stop(ui);
                SmokePhase::Capture(now + Duration::from_millis(100))
            }
            SmokePhase::Capture(ready_at) if now >= ready_at => {
                if let Some(path) = &smoke.screenshot {
                    let pixels = ui.window().take_snapshot().map_err(anyhow::Error::msg)?;
                    let rgb: Vec<_> = pixels
                        .as_slice()
                        .iter()
                        .flat_map(|pixel| [pixel.r, pixel.g, pixel.b])
                        .collect();
                    image::save_buffer(
                        path,
                        &rgb,
                        pixels.width(),
                        pixels.height(),
                        image::ColorType::Rgb8,
                    )?;
                }
                slint::quit_event_loop().map_err(anyhow::Error::msg)?;
                self.smoke = None;
                return Ok(true);
            }
            _ => return Ok(false),
        };
        self.smoke.as_mut().unwrap().phase = phase;
        Ok(false)
    }

    fn toggle_measurement(&mut self, ui: &EditorWindow) {
        self.metrics.enabled = !self.metrics.enabled;
        self.metrics.previous_tick = None;
        ui.set_measuring(self.metrics.enabled);
    }

    fn save_metrics(&mut self, ui: &EditorWindow) -> Result<()> {
        self.metrics.enabled = false;
        ui.set_measuring(false);
        if let Some(path) = json_dialog("計測を保存")
            .set_file_name("preview-metrics.json")
            .save_file()
        {
            serde_json::to_writer_pretty(
                File::create(path)?,
                &json!({
                    "version":1,
                    "truncated_events":self.metrics.dropped,
                    "events":self.metrics.events,
                }),
            )?;
            ui.set_status("計測ファイルを保存しました".into());
        }
        Ok(())
    }
}

fn timestamp(micros: i64) -> String {
    let millis = micros / 1_000;
    format!(
        "{:02}:{:02}.{:03}",
        millis / 60_000,
        millis / 1_000 % 60,
        millis % 1_000
    )
}

fn seconds_text(micros: i64) -> String {
    format!("{}.{:06}", micros / SECOND, micros % SECOND)
}

fn caption_row(caption: &meditor_playback::project::Caption) -> CaptionRow {
    CaptionRow {
        time_label: timestamp(caption.start_us).into(),
        speaker: caption.speaker.as_str().into(),
        body: caption.text.lines().next().unwrap_or_default().into(),
    }
}

fn json_dialog(title: &str) -> rfd::FileDialog {
    rfd::FileDialog::new()
        .set_title(title)
        .add_filter("JSON", &["json"])
}

fn video_dialog(title: &str) -> rfd::FileDialog {
    rfd::FileDialog::new()
        .set_title(title)
        .add_filter("動画", &["mp4", "mkv", "mov", "webm"])
}

fn bind(ui: &EditorWindow, state: &Rc<RefCell<Editor>>) {
    macro_rules! callback {
        ($registration:ident, |$editor:ident, $ui:ident $(, $arg:ident)?| $action:expr) => {{
            let window = ui.as_weak();
            let state = state.clone();
            ui.$registration(move |$($arg)?| {
                if let Some($ui) = window.upgrade() {
                    let mut $editor = state.borrow_mut();
                    let result: Result<()> = $action;
                    if let Err(error) = result {
                        $ui.set_status(format!("{error:#}").into());
                    }
                }
            });
        }};
    }

    callback!(on_open_project, |editor, ui| editor
        .request_leave(&ui, Leave::Open));
    callback!(on_new_project, |editor, ui| editor
        .request_leave(&ui, Leave::New));
    callback!(on_save_project, |editor, ui| editor.save(&ui));
    callback!(on_import_captions, |editor, ui| editor.import(&ui));
    callback!(on_choose_proxy, |editor, ui| editor.choose_proxy(&ui));
    callback!(on_update_clip, |editor, ui| editor.edit_clip(&ui));
    callback!(on_save_metrics, |editor, ui| editor.save_metrics(&ui));
    callback!(on_toggle_measurement, |editor, ui| {
        editor.toggle_measurement(&ui);
        Ok(())
    });
    callback!(on_toggle_playback, |editor, ui| {
        editor.toggle_play(&ui);
        Ok(())
    });
    callback!(on_select_caption, |editor, ui, index| {
        editor.select(&ui, index);
        Ok(())
    });
    callback!(on_goto_caption, |editor, ui, index| {
        if let Some(position) = editor.select(&ui, index) {
            editor.seek_original(&ui, position, false);
        }
        Ok(())
    });
    callback!(on_update_caption, |editor, ui| editor.edit_caption(&ui));
    callback!(on_undo_edit, |editor, ui| editor.replay_history(&ui, false));
    callback!(on_redo_edit, |editor, ui| editor.replay_history(&ui, true));
    callback!(on_scrub, |editor, ui, value| {
        editor.scrub(&ui, value);
        Ok(())
    });
    callback!(on_jump_to, |editor, ui, value| {
        if let Some(loaded) = &editor.loaded {
            let position = loaded.range().duration() * i64::from(value) / SLIDER_RANGE;
            let resume = editor.playing || editor.resume_pending;
            editor.seek_original(&ui, position, resume);
        }
        Ok(())
    });
    callback!(on_finish_scrub, |editor, ui, value| {
        editor.finish_scrub(&ui, value);
        Ok(())
    });
    callback!(on_confirm_leave, |editor, ui| {
        ui.set_confirm_discard(false);
        match editor.leaving.take() {
            Some(leaving) => editor.leave(&ui, leaving),
            None => Ok(()),
        }
    });
    callback!(on_save_and_leave, |editor, ui| {
        match editor.save(&ui) {
            Ok(()) => {
                ui.set_confirm_discard(false);
                match editor.leaving.take() {
                    Some(leaving) => editor.leave(&ui, leaving),
                    None => Ok(()),
                }
            }
            Err(error) => {
                ui.set_confirm_discard(false);
                editor.leaving = None;
                Err(error)
            }
        }
    });
    callback!(on_cancel_leave, |editor, ui| {
        ui.set_confirm_discard(false);
        editor.leaving = None;
        Ok(())
    });
    let window = ui.as_weak();
    let state = state.clone();
    ui.window().on_close_requested(move || {
        if let Some(ui) = window.upgrade() {
            let mut editor = state.borrow_mut();
            if editor.loaded.as_ref().is_some_and(|loaded| loaded.dirty) {
                editor.leaving = Some(Leave::Quit);
                ui.set_confirm_discard(true);
                return slint::CloseRequestResponse::KeepWindowShown;
            }
        }
        slint::CloseRequestResponse::HideWindow
    });
}

struct Launch {
    project: Option<PathBuf>,
    smoke: Option<Smoke>,
}

fn arguments(mut args: impl Iterator<Item = OsString>) -> Result<Launch> {
    let Some(first) = args.next() else {
        return Ok(Launch {
            project: None,
            smoke: None,
        });
    };
    let launch = if first == "--smoke-test" {
        Launch {
            project: Some(args.next().context("--smoke-test PROJECT [PNG]")?.into()),
            smoke: Some(Smoke {
                screenshot: args.next().map(PathBuf::from),
                deadline: Instant::now() + Duration::from_secs(45),
                phase: SmokePhase::FirstFrame,
            }),
        }
    } else {
        Launch {
            project: Some(first.into()),
            smoke: None,
        }
    };
    ensure!(
        args.next().is_none(),
        "meditor [PROJECT] または meditor --smoke-test PROJECT [PNG]"
    );
    Ok(launch)
}

fn main() -> Result<()> {
    ffmpeg_next::init()?;
    let launch = arguments(std::env::args_os().skip(1))?;
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .select()
        .map_err(anyhow::Error::msg)?;
    let ui = EditorWindow::new().map_err(anyhow::Error::msg)?;
    let state = Rc::new(RefCell::new(Editor {
        smoke: launch.smoke,
        ..Editor::default()
    }));
    if let Some(path) = launch.project {
        let opened = Project::load(&path).and_then(|project| Loaded::open(project, path));
        match opened {
            Ok(loaded) => {
                let mut editor = state.borrow_mut();
                editor.install(&ui, loaded);
                if editor.smoke.is_some() {
                    editor.select(&ui, 0);
                }
            }
            Err(error) if state.borrow().smoke.is_some() => return Err(error),
            Err(error) => ui.set_status(format!("{error:#}").into()),
        }
    }
    bind(&ui, &state);
    let error = Rc::new(RefCell::new(None));
    let timer = Timer::default();
    {
        let window = ui.as_weak();
        let state = state.clone();
        let error = error.clone();
        timer.start(TimerMode::Repeated, TICK, move || {
            if let Some(ui) = window.upgrade() {
                let mut editor = state.borrow_mut();
                if let Err(failure) = editor.tick(&ui) {
                    if editor.smoke.is_some() {
                        *error.borrow_mut() = Some(failure);
                        let _ = slint::quit_event_loop();
                    } else {
                        editor.stop(&ui);
                        if let Some(loaded) = &mut editor.loaded {
                            loaded.audio = None;
                        }
                        ui.set_status(format!("{failure:#}").into());
                    }
                }
            }
        });
    }
    ui.run().map_err(anyhow::Error::msg)?;
    if let Some(failure) = error.borrow_mut().take() {
        return Err(failure);
    }
    Ok(())
}
