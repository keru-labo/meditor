use anyhow::{Result, ensure};
use meditor_playback::project::{Caption, ClipRange};
use std::sync::mpsc::{self, Receiver};

pub struct SelectionInput {
    pub bounds: ClipRange,
    pub captions: Vec<Caption>,
    pub clip_draft: [String; 2],
}

impl SelectionInput {
    pub fn matches(
        &self,
        bounds: ClipRange,
        captions: &[Caption],
        clip_draft: &[String; 2],
    ) -> bool {
        self.bounds == bounds && self.captions == captions && &self.clip_draft == clip_draft
    }
}

pub struct SelectionResult {
    pub input: SelectionInput,
    pub range: Result<Option<ClipRange>>,
}

struct PendingSelection {
    result: Receiver<SelectionResult>,
    accept: bool,
}

#[derive(Default)]
pub struct ChatSelection(Option<PendingSelection>);

impl ChatSelection {
    pub fn is_running(&self) -> bool {
        self.0.is_some()
    }

    pub fn invalidate(&mut self) -> bool {
        self.0
            .as_mut()
            .is_some_and(|pending| std::mem::take(&mut pending.accept))
    }

    pub fn start(
        &mut self,
        input: SelectionInput,
        select: impl FnOnce(&SelectionInput) -> Result<Option<ClipRange>> + Send + 'static,
    ) -> Result<()> {
        ensure!(
            !self.is_running(),
            "前の AI 処理が終了するまでお待ちください"
        );
        let (send, result) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("ai-clip-selection".into())
            .spawn(move || {
                let range = select(&input);
                let _ = send.send(SelectionResult { input, range });
            })?;
        self.0 = Some(PendingSelection {
            result,
            accept: true,
        });
        Ok(())
    }

    pub fn poll(&mut self) -> Result<Option<SelectionResult>> {
        let Some(pending) = &self.0 else {
            return Ok(None);
        };
        match pending.result.try_recv() {
            Ok(result) => {
                let accept = pending.accept;
                self.0 = None;
                Ok(accept.then_some(result))
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                let accept = pending.accept;
                self.0 = None;
                ensure!(!accept, "AI 処理が予期せず終了しました");
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(start: i64, end: i64) -> ClipRange {
        ClipRange {
            source_start_us: start,
            source_end_us: end,
        }
    }

    #[test]
    fn cancelled_selection_keeps_worker_slot_and_discards_its_result() {
        let (send, result) = mpsc::sync_channel(1);
        let mut selection = ChatSelection(Some(PendingSelection {
            result,
            accept: true,
        }));
        assert!(selection.invalidate());
        assert!(!selection.invalidate());
        assert!(selection.is_running());
        let input = SelectionInput {
            bounds: range(0, 40),
            captions: vec![],
            clip_draft: ["0".into(), "40".into()],
        };
        send.send(SelectionResult {
            input,
            range: Ok(Some(range(20, 30))),
        })
        .ok()
        .unwrap();
        assert!(selection.poll().unwrap().is_none());
        assert!(!selection.is_running());
    }

    #[test]
    fn captured_captions_and_draft_must_still_match() {
        let caption = Caption {
            id: "1".into(),
            start_us: 10,
            end_us: 20,
            speaker: String::new(),
            text: "元の字幕".into(),
        };
        let bounds = range(0, 40);
        let draft = ["0".into(), "40".into()];
        let input = SelectionInput {
            bounds,
            captions: vec![caption.clone()],
            clip_draft: draft.clone(),
        };
        assert!(input.matches(bounds, std::slice::from_ref(&caption), &draft));
        assert!(!input.matches(range(10, 40), std::slice::from_ref(&caption), &draft));
        assert!(!input.matches(
            bounds,
            std::slice::from_ref(&caption),
            &["1".into(), "40".into()]
        ));
        assert!(!input.matches(
            bounds,
            &[Caption {
                text: "変更した字幕".into(),
                ..caption
            }],
            &draft
        ));
    }
}
