use anyhow::{Result, ensure};
use meditor_playback::{
    ai::ClipSelection,
    project::{Caption, ClipRange},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
};

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
    pub selection: Result<Option<ClipSelection>>,
}

struct PendingSelection {
    result: Receiver<SelectionResult>,
    cancelled: Arc<AtomicBool>,
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
            .is_some_and(|pending| !pending.cancelled.swap(true, Ordering::Relaxed))
    }

    pub fn start(
        &mut self,
        input: SelectionInput,
        select: impl FnOnce(&SelectionInput, &AtomicBool) -> Result<Option<ClipSelection>>
        + Send
        + 'static,
    ) -> Result<()> {
        ensure!(
            !self.is_running(),
            "前の AI 処理が終了するまでお待ちください"
        );
        let (send, result) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        std::thread::Builder::new()
            .name("ai-clip-selection".into())
            .spawn(move || {
                let selection = select(&input, &worker_cancelled);
                let _ = send.send(SelectionResult { input, selection });
            })?;
        self.0 = Some(PendingSelection { result, cancelled });
        Ok(())
    }

    pub fn poll(&mut self) -> Result<Option<SelectionResult>> {
        let Some(pending) = &self.0 else {
            return Ok(None);
        };
        match pending.result.try_recv() {
            Ok(result) => {
                let accept = !pending.cancelled.load(Ordering::Relaxed);
                self.0 = None;
                Ok(accept.then_some(result))
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                let accept = !pending.cancelled.load(Ordering::Relaxed);
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
    use meditor_playback::{
        history::{Edit, EditHistory, EditTarget},
        project::Project,
    };
    use serde::Deserialize;

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
            cancelled: Arc::new(AtomicBool::new(false)),
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
            selection: Ok(Some(ClipSelection {
                range: range(20, 30),
                export: None,
            })),
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

    #[test]
    fn replay_quint_cancelled_reply_after_caption_edit() -> Result<()> {
        #[derive(Deserialize)]
        struct Tag {
            tag: String,
        }

        #[derive(Deserialize)]
        struct EditState {
            caption: bool,
            clip: Tag,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct State {
            last_action: String,
            request: Tag,
            reply: Tag,
            edit: EditState,
        }

        #[derive(Deserialize)]
        struct Step {
            state: State,
        }

        #[derive(Deserialize)]
        struct Trace {
            states: Vec<Step>,
        }

        let trace: Trace = serde_json::from_str(include_str!(
            "../../verification/quint/traces/chat_edit_cancelled.itf.json"
        ))?;
        let commands: Vec<_> = trace
            .states
            .iter()
            .map(|step| step.state.last_action.as_str())
            .collect();
        assert_eq!(
            commands,
            ["init", "submit", "cancel", "editCaption", "arrive", "poll"]
        );

        let mut chat = ChatSelection::default();
        let mut project = Project::new("video.mp4".into(), vec![])?;
        project.captions.push(Caption {
            id: "1".into(),
            start_us: 20,
            end_us: 30,
            speaker: String::new(),
            text: "元の字幕".into(),
        });
        let mut history = EditHistory::default();
        let mut reply = None;
        for (index, Step { state }) in trace.states.into_iter().enumerate() {
            let action = state.last_action.as_str();
            match action {
                "init" => (),
                "submit" => {
                    // Control the worker's completion order while exercising the real receiver.
                    let (send, result) = mpsc::sync_channel(1);
                    chat.0 = Some(PendingSelection {
                        result,
                        cancelled: Arc::new(AtomicBool::new(false)),
                    });
                    reply = Some((
                        send,
                        SelectionInput {
                            bounds: range(0, 40),
                            captions: project.captions.clone(),
                            clip_draft: ["0".into(), "40".into()],
                        },
                    ));
                }
                "cancel" => assert!(chat.invalidate()),
                "editCaption" => {
                    assert!(matches!(
                        history.apply(
                            &mut project,
                            40,
                            Edit::Caption {
                                row: 0,
                                id: "1".into(),
                                speaker: String::new(),
                                text: "変更した字幕".into(),
                            }
                        )?,
                        Some(EditTarget::Caption(0))
                    ));
                }
                "arrive" => {
                    assert_eq!(state.reply.tag, "SelectedClip", "ITF worker response");
                    let (send, input) = reply.take().expect("pending model reply");
                    assert!(
                        send.send(SelectionResult {
                            input,
                            selection: Ok(Some(ClipSelection {
                                range: range(20, 30),
                                export: None
                            })),
                        })
                        .is_ok()
                    );
                }
                "poll" => assert!(chat.poll()?.is_none(), "cancelled response was returned"),
                _ => anyhow::bail!("unknown model command {action}"),
            }
            let request = match &chat.0 {
                None => "Idle",
                Some(pending) if pending.cancelled.load(Ordering::Relaxed) => "Cancelled",
                Some(_) => "Pending",
            };
            assert_eq!(
                request, state.request.tag,
                "ITF state {index} after {action}"
            );
            let expected_caption = if state.edit.caption {
                "変更した字幕"
            } else {
                "元の字幕"
            };
            assert_eq!(
                project.captions[0].text, expected_caption,
                "ITF state {index} after {action}"
            );
            assert_eq!(state.edit.clip.tag, "NoClip");
            assert_eq!(project.clip, None, "ITF state {index} after {action}");
        }
        assert!(matches!(
            history.undo(&mut project, 40)?,
            Some(EditTarget::Caption(0))
        ));
        assert!(!history.can_undo());
        Ok(())
    }
}
