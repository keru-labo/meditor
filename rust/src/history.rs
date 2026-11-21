//! Reversible caption and clip edits.

use crate::project::{ClipRange, Project};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::value::{RawValue, to_raw_value};
use std::{collections::HashMap, mem};

const ARCHIVE_VERSION: u32 = 1;
const MAX_SAVED_EDITS_PER_STACK: usize = 128;
const MAX_SAVED_STACK_BYTES: usize = 512 * 1024;
const MAX_ARCHIVE_BYTES: usize = 2 * MAX_SAVED_STACK_BYTES + 64;

#[derive(Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Edit {
    Caption {
        row: usize,
        id: String,
        speaker: String,
        text: String,
    },
    Clip(Option<ClipRange>),
    ClipWithCaptionBasis {
        #[serde(deserialize_with = "Option::deserialize")]
        range: Option<ClipRange>,
        captions_use_source_time: bool,
    },
}

pub enum EditTarget {
    Caption(usize),
    Clip,
}

impl Edit {
    // The same operation can be replayed in both directions because it retains
    // the previous value after each application.
    fn exchange(&mut self, project: &mut Project, duration: i64) -> Result<Option<EditTarget>> {
        match self {
            Self::Caption {
                row,
                id,
                speaker,
                text,
            } => {
                let caption = project
                    .captions
                    .get_mut(*row)
                    .filter(|caption| caption.id == *id)
                    .context("対象の字幕が見つかりません")?;
                if caption.speaker == *speaker && caption.text == *text {
                    return Ok(None);
                }
                mem::swap(&mut caption.speaker, speaker);
                mem::swap(&mut caption.text, text);
                Ok(Some(EditTarget::Caption(*row)))
            }
            Self::Clip(range) => {
                if project.clip == *range {
                    return Ok(None);
                }
                let previous = project.clip;
                project.set_clip_override(*range, duration)?;
                *range = previous;
                Ok(Some(EditTarget::Clip))
            }
            Self::ClipWithCaptionBasis {
                range,
                captions_use_source_time,
            } => {
                if project.clip == *range
                    && project.captions_use_source_time == *captions_use_source_time
                {
                    return Ok(None);
                }
                let previous = project.clip;
                project.set_clip_override(*range, duration)?;
                *range = previous;
                mem::swap(
                    &mut project.captions_use_source_time,
                    captions_use_source_time,
                );
                Ok(Some(EditTarget::Clip))
            }
        }
    }
}

#[derive(Default)]
pub struct EditHistory {
    undo: Vec<Edit>,
    redo: Vec<Edit>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredHistory {
    version: u32,
    undo: Vec<Edit>,
    redo: Vec<Edit>,
}

#[derive(Serialize)]
struct HistoryArchive<'a> {
    version: u32,
    undo: &'a [Edit],
    redo: &'a [Edit],
}

impl EditHistory {
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub(crate) fn archive(&self) -> Result<Option<Box<RawValue>>> {
        let undo = retained_edits(&self.undo)?;
        let redo = retained_edits(&self.redo)?;
        if undo.is_empty() && redo.is_empty() {
            return Ok(None);
        }
        Ok(Some(to_raw_value(&HistoryArchive {
            version: ARCHIVE_VERSION,
            undo,
            redo,
        })?))
    }

    pub(crate) fn restore(archive: &RawValue, project: &Project, duration: i64) -> Result<Self> {
        ensure!(
            archive.get().len() <= MAX_ARCHIVE_BYTES,
            "作業履歴が大きすぎます"
        );
        let history: StoredHistory =
            serde_json::from_str(archive.get()).context("作業履歴を読み込めません")?;
        ensure!(history.version == ARCHIVE_VERSION, "未対応の作業履歴です");
        validate_sequence(&history.undo, project, duration)?;
        validate_sequence(&history.redo, project, duration)?;
        Ok(Self {
            undo: history.undo,
            redo: history.redo,
        })
    }

    pub fn apply(
        &mut self,
        project: &mut Project,
        duration: i64,
        mut edit: Edit,
    ) -> Result<Option<EditTarget>> {
        let target = edit.exchange(project, duration)?;
        if target.is_some() {
            self.undo.push(edit);
            self.redo.clear();
        }
        Ok(target)
    }

    pub fn undo(&mut self, project: &mut Project, duration: i64) -> Result<Option<EditTarget>> {
        Self::replay(&mut self.undo, &mut self.redo, project, duration)
    }

    pub fn redo(&mut self, project: &mut Project, duration: i64) -> Result<Option<EditTarget>> {
        Self::replay(&mut self.redo, &mut self.undo, project, duration)
    }

    fn replay(
        from: &mut Vec<Edit>,
        to: &mut Vec<Edit>,
        project: &mut Project,
        duration: i64,
    ) -> Result<Option<EditTarget>> {
        let Some(edit) = from.last_mut() else {
            return Ok(None);
        };
        let target = edit.exchange(project, duration)?;
        ensure!(target.is_some(), "対象はすでに変更先の状態です");
        to.push(from.pop().expect("replaying a non-empty history"));
        Ok(target)
    }
}

fn retained_edits(edits: &[Edit]) -> Result<&[Edit]> {
    let mut start = edits.len();
    let mut remaining = MAX_SAVED_STACK_BYTES;
    for edit in edits.iter().rev().take(MAX_SAVED_EDITS_PER_STACK) {
        // Skip serialization if the raw text alone exceeds the byte budget.
        if let Edit::Caption {
            id, speaker, text, ..
        } = edit
            && id
                .len()
                .saturating_add(speaker.len())
                .saturating_add(text.len())
                >= remaining
        {
            break;
        }
        let encoded_size = serde_json::to_vec(edit)?.len() + 1; // Include a possible comma.
        if encoded_size > remaining {
            break;
        }
        remaining -= encoded_size;
        start -= 1;
    }
    Ok(&edits[start..])
}

fn validate_sequence(edits: &[Edit], project: &Project, duration: i64) -> Result<()> {
    ensure!(
        edits.len() <= MAX_SAVED_EDITS_PER_STACK,
        "作業履歴の件数が上限を超えました"
    );
    let mut clip = project.clip;
    let mut captions_use_source_time = project.captions_use_source_time;
    let mut captions = HashMap::new();
    for edit in edits.iter().rev() {
        match edit {
            Edit::Caption {
                row,
                id,
                speaker,
                text,
            } => {
                let source = project
                    .captions
                    .get(*row)
                    .filter(|caption| caption.id == *id)
                    .context("作業履歴の対象字幕が見つかりません")?;
                let current = captions
                    .entry(*row)
                    .or_insert((&source.speaker, &source.text));
                let next = (speaker, text);
                ensure!(*current != next, "作業履歴の変更前後が同じです");
                *current = next;
            }
            Edit::Clip(range) => {
                ensure!(
                    clip != *range && range.is_none_or(|range| range.valid_for(duration)),
                    "作業履歴の素材区間が無効です"
                );
                clip = *range;
            }
            Edit::ClipWithCaptionBasis {
                range,
                captions_use_source_time: next_basis,
            } => {
                ensure!(
                    (clip != *range || captions_use_source_time != *next_basis)
                        && range.is_none_or(|range| range.valid_for(duration)),
                    "作業履歴の素材区間または字幕時刻の基準が無効です"
                );
                clip = *range;
                captions_use_source_time = *next_basis;
            }
        }
    }
    Ok(())
}
