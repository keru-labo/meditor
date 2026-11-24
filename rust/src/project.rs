use crate::{history::EditHistory, subtitle_style::SubtitleStyle};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::{
    collections::HashSet,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::OnceLock,
};

pub const SECOND: i64 = 1_000_000;
pub const MAX_DURATION: i64 = 24 * 60 * 60 * SECOND;
const MAX_CAPTIONS: usize = 1_000_000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Caption {
    pub id: String,
    pub start_us: i64,
    pub end_us: i64,
    pub speaker: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub path: PathBuf,
    pub name: String,
    #[serde(default)]
    pub offset_us: i64,
    #[serde(default = "unity_gain")]
    pub gain: f64,
}

fn unity_gain() -> f64 {
    1.0
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClipRange {
    pub source_start_us: i64,
    pub source_end_us: i64,
}

impl ClipRange {
    pub fn valid_for(self, duration: i64) -> bool {
        matches!(self.source_start_us, 0..=MAX_DURATION)
            && self.source_start_us < self.source_end_us
            && self.source_end_us <= duration.min(MAX_DURATION)
    }

    pub fn duration(self) -> i64 {
        self.source_end_us - self.source_start_us
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Project {
    version: u32,
    pub video: PathBuf,
    #[serde(default)]
    pub proxy: PathBuf,
    pub audio: Vec<Track>,
    pub captions: Vec<Caption>,
    #[serde(default, skip_serializing_if = "SubtitleStyle::is_default")]
    pub subtitle_style: SubtitleStyle,
    #[serde(default, skip_serializing_if = "is_false")]
    pub captions_use_source_time: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip: Option<ClipRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    edit_history: Option<Box<RawValue>>,
}

impl Project {
    pub fn new(video: PathBuf, audio: Vec<Track>) -> Result<Self> {
        let project = Self {
            version: 1,
            video,
            proxy: PathBuf::new(),
            audio,
            captions: Vec::new(),
            subtitle_style: SubtitleStyle::default(),
            captions_use_source_time: false,
            clip: None,
            edit_history: None,
        };
        project.validate()?;
        Ok(project)
    }

    pub fn load(path: &Path) -> Result<Self> {
        let path = std::path::absolute(path)?;
        let directory = path
            .parent()
            .context("プロジェクトのフォルダーがありません")?;
        let mut project: Self = serde_json::from_reader(File::open(&path)?)
            .context("プロジェクト JSON を読み込めません")?;
        project.validate()?;
        for media in project.media_paths_mut() {
            if !media.as_os_str().is_empty() {
                *media = directory.join(&*media);
            }
        }
        project.captions.sort_by_key(|caption| caption.start_us);
        Ok(project)
    }

    pub fn save(&mut self, path: &Path) -> Result<()> {
        self.validate()?;
        let path = std::path::absolute(path)?;
        let directory = path
            .parent()
            .context("プロジェクトのフォルダーがありません")?;
        let mut staged = tempfile::NamedTempFile::new_in(directory)?;
        let media_paths = self
            .media_paths_mut()
            .map(|media| {
                if media.is_absolute() {
                    let relative = pathdiff::diff_paths(&*media, directory);
                    if let Some(relative) = relative {
                        return Some(std::mem::replace(media, relative));
                    }
                }
                None
            })
            .collect::<Vec<_>>();
        let result = serde_json::to_writer_pretty(&mut staged, self);
        for (media, original) in self.media_paths_mut().zip(media_paths) {
            if let Some(original) = original {
                *media = original;
            }
        }
        result?;
        staged.write_all(b"\n")?;
        staged.as_file().sync_all()?;
        staged.persist(&path)?;
        Ok(())
    }

    pub fn set_clip(&mut self, range: ClipRange, duration: i64) -> Result<()> {
        self.set_clip_override(Some(range), duration)
    }

    pub fn take_edit_history(&mut self, duration: i64) -> Result<EditHistory> {
        match self.edit_history.take() {
            Some(archive) => EditHistory::restore(&archive, self, duration),
            None => Ok(EditHistory::default()),
        }
    }

    pub fn save_with_history(&mut self, path: &Path, history: &EditHistory) -> Result<()> {
        let archive = history.archive()?;
        let previous = std::mem::replace(&mut self.edit_history, archive);
        let result = self.save(path);
        self.edit_history = previous;
        result
    }

    pub(crate) fn set_clip_override(
        &mut self,
        range: Option<ClipRange>,
        duration: i64,
    ) -> Result<()> {
        ensure!(
            range.is_none_or(|range| range.valid_for(duration)),
            "素材内の有効な開始・終了時刻を入力してください"
        );
        self.clip = range;
        self.version = if range.is_some() { 2 } else { 1 };
        Ok(())
    }

    pub fn playback_range(&self, duration: i64) -> ClipRange {
        self.clip.unwrap_or(ClipRange {
            source_start_us: 0,
            source_end_us: duration,
        })
    }

    /// Add this offset to a playhead position to look up a stored caption.
    /// Subtract it from a stored caption time to place the caption on the timeline.
    pub fn caption_time_offset(&self, range: ClipRange) -> i64 {
        if self.captions_use_source_time {
            range.source_start_us
        } else {
            0
        }
    }

    fn media_paths_mut(&mut self) -> impl Iterator<Item = &mut PathBuf> {
        [&mut self.video, &mut self.proxy]
            .into_iter()
            .chain(self.audio.iter_mut().map(|track| &mut track.path))
    }

    fn validate(&self) -> Result<()> {
        ensure!(!self.video.as_os_str().is_empty(), "元動画が必要です");
        self.subtitle_style.validate()?;
        match (self.version, self.clip) {
            (1, None) => {}
            (2, Some(range)) => ensure!(range.valid_for(MAX_DURATION), "素材区間が無効です"),
            _ => anyhow::bail!("未対応のプロジェクト形式です"),
        }
        ensure!(self.audio.len() <= 32, "音声は32トラックまでです");
        for track in &self.audio {
            ensure!(
                !track.path.as_os_str().is_empty()
                    && (-MAX_DURATION..=MAX_DURATION).contains(&track.offset_us)
                    && (0.0..=4.0).contains(&track.gain),
                "無効な音声トラックです"
            );
        }
        ensure!(
            self.captions.len() <= MAX_CAPTIONS,
            "字幕件数が上限を超えました"
        );
        let mut ids = HashSet::with_capacity(self.captions.len());
        for caption in &self.captions {
            ensure!(
                caption.start_us >= 0
                    && caption.start_us < caption.end_us
                    && caption.end_us <= MAX_DURATION
                    && !caption.id.is_empty()
                    && ids.insert(&caption.id),
                "字幕の時刻または ID が無効です"
            );
        }
        Ok(())
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

pub fn import_srt(path: &Path) -> Result<Vec<Caption>> {
    let input = fs::read_to_string(path)?;
    let mut lines = input.trim_start_matches('\u{feff}').lines().peekable();
    static INTERVAL: OnceLock<regex::Regex> = OnceLock::new();
    let interval = INTERVAL.get_or_init(|| {
        let time = r"([0-9]{2}):([0-5][0-9]):([0-5][0-9]),([0-9]{3})";
        regex::Regex::new(&format!(r"^{time}[ \t]+-->[ \t]+{time}(?:[ \t].*)?$")).unwrap()
    });
    let mut captions = Vec::new();
    while let Some(number) = lines.next() {
        if number.is_empty() {
            continue;
        }
        ensure!(
            number.bytes().all(|c| c.is_ascii_digit()),
            "SRT の字幕番号が無効です"
        );
        let time = lines
            .next()
            .and_then(|line| interval.captures(line))
            .context("SRT の字幕時刻が無効です")?;
        let micros = |index: usize| {
            let part = |n: usize| time[n].parse::<i64>().unwrap();
            (part(index) * 3600 + part(index + 1) * 60 + part(index + 2)) * SECOND
                + part(index + 3) * 1000
        };
        let (start_us, end_us) = (micros(1), micros(5));
        ensure!(
            start_us < end_us && end_us <= MAX_DURATION,
            "SRT の字幕区間が無効です"
        );
        ensure!(captions.len() < MAX_CAPTIONS, "字幕件数が上限を超えました");
        let mut text = String::new();
        while let Some(line) = lines.next_if(|line| !line.is_empty()) {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(line);
        }
        captions.push(Caption {
            id: (captions.len() + 1).to_string(),
            start_us,
            end_us,
            speaker: String::new(),
            text,
        });
    }
    captions.sort_by_key(|caption| caption.start_us);
    Ok(captions)
}

pub fn parse_seconds(input: &str) -> Option<i64> {
    let input = input.trim();
    let (whole, fraction) = match input.split_once('.') {
        Some((_, "")) => return None,
        Some(parts) => parts,
        None => (input, ""),
    };
    if whole.is_empty()
        || !whole.bytes().all(|c| c.is_ascii_digit())
        || fraction.len() > 6
        || !fraction.bytes().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    let whole = whole.parse::<i64>().ok()?.checked_mul(SECOND)?;
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<i64>().ok()? * 10_i64.pow(6 - fraction.len() as u32)
    };
    whole
        .checked_add(fraction)
        .filter(|value| *value <= MAX_DURATION)
}
