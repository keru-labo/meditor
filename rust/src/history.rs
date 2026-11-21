//! Reversible edits for the current project session.

use crate::project::{ClipRange, Project};
use anyhow::{Context, Result, ensure};
use std::mem;

pub enum Edit {
    Caption {
        row: usize,
        id: String,
        speaker: String,
        text: String,
    },
    Clip(Option<ClipRange>),
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
        }
    }
}

#[derive(Default)]
pub struct EditHistory {
    undo: Vec<Edit>,
    redo: Vec<Edit>,
}

impl EditHistory {
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
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
