use anyhow::Result;
use meditor_playback::{
    history::{Edit, EditHistory, EditTarget},
    project::{ClipRange, Project, SECOND},
    subtitle_style::SubtitleStyle,
};
use serde_json::{Value, json};
use std::fs;

#[path = "support/history.rs"]
mod fixture;

use fixture::{DURATION, TRIM, caption, project};

#[test]
fn subtitle_style_is_persistent_and_reversible_with_existing_edits() -> Result<()> {
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("project.json");
    let mut project = project()?;
    let mut history = EditHistory::default();
    let style = SubtitleStyle {
        font_family: "Yu Gothic UI".into(),
        font_size: 38,
        text_color: [255, 220, 0],
        outline_color: [0, 0, 255],
        outline_width: 0,
    };
    history.apply(&mut project, DURATION, caption("ボス戦"))?;
    history.apply(&mut project, DURATION, Edit::Clip(Some(TRIM)))?;
    assert!(matches!(
        history.apply(&mut project, DURATION, Edit::SubtitleStyle(style.clone()))?,
        Some(EditTarget::SubtitleStyle)
    ));
    project.save_with_history(&path, &history)?;

    let mut project = Project::load(&path)?;
    let mut history = project.take_edit_history(DURATION)?;
    assert_eq!(project.subtitle_style, style);
    assert!(matches!(
        history.undo(&mut project, DURATION)?,
        Some(EditTarget::SubtitleStyle)
    ));
    assert_eq!(project.subtitle_style, SubtitleStyle::default());
    assert_eq!(project.clip, Some(TRIM));
    assert_eq!(project.captions[0].text, "ボス戦");
    project.save_with_history(&path, &history)?;
    let saved: Value = serde_json::from_slice(&fs::read(&path)?)?;
    assert!(saved.get("subtitle_style").is_none());

    let mut project = Project::load(&path)?;
    let mut history = project.take_edit_history(DURATION)?;
    assert_eq!(project.subtitle_style, SubtitleStyle::default());
    assert!(matches!(
        history.redo(&mut project, DURATION)?,
        Some(EditTarget::SubtitleStyle)
    ));
    assert_eq!(project.subtitle_style, style);
    assert!(matches!(
        history.undo(&mut project, DURATION)?,
        Some(EditTarget::SubtitleStyle)
    ));
    assert!(matches!(
        history.undo(&mut project, DURATION)?,
        Some(EditTarget::Clip)
    ));
    assert!(matches!(
        history.undo(&mut project, DURATION)?,
        Some(EditTarget::Caption(0))
    ));
    Ok(())
}

#[test]
fn invalid_subtitle_style_edits_cannot_replace_the_project_or_its_history() -> Result<()> {
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("project.json");
    let mut project = project()?;
    let mut history = EditHistory::default();
    history.apply(&mut project, DURATION, Edit::Clip(Some(TRIM)))?;
    history.undo(&mut project, DURATION)?;
    for (font_family, font_size, outline_width) in [
        ("", 24, 2),
        (" Arial ", 24, 2),
        ("Arial,Style", 24, 2),
        ("Arial\nDialogue", 24, 2),
        ("Arial", 7, 2),
        ("Arial", 97, 2),
        ("Arial", 24, 13),
    ] {
        let bad = SubtitleStyle {
            font_family: font_family.into(),
            font_size,
            outline_width,
            ..SubtitleStyle::default()
        };
        assert!(
            history
                .apply(&mut project, DURATION, Edit::SubtitleStyle(bad))
                .is_err()
        );
        assert_eq!(project.subtitle_style, SubtitleStyle::default());
        assert!(history.can_redo());
    }

    project.save_with_history(&path, &history)?;
    let mut valid: Value = serde_json::from_slice(&fs::read(&path)?)?;
    assert_eq!(
        Project::load(&path)?.subtitle_style,
        SubtitleStyle::default()
    );
    let bad = SubtitleStyle {
        font_family: "\\fnUnexpected".into(),
        ..SubtitleStyle::default()
    };
    valid["edit_history"]["undo"] = json!([{"kind":"subtitle_style","value":bad}]);
    fs::write(&path, serde_json::to_vec(&valid)?)?;
    let mut reopened = Project::load(&path)?;
    assert!(reopened.take_edit_history(DURATION).is_err());
    assert_eq!(reopened.subtitle_style, SubtitleStyle::default());

    valid["subtitle_style"] = serde_json::to_value(bad)?;
    fs::write(&path, serde_json::to_vec(&valid)?)?;
    assert!(Project::load(&path).is_err());
    Ok(())
}

#[test]
fn caption_and_clip_undo_and_redo_follow_the_same_order() -> Result<()> {
    let mut project = project()?;
    let mut history = EditHistory::default();
    assert!(
        history
            .apply(&mut project, DURATION, caption("ボス戦"))?
            .is_some()
    );
    assert!(
        history
            .apply(&mut project, DURATION, Edit::Clip(Some(TRIM)))?
            .is_some()
    );

    assert!(matches!(
        history.undo(&mut project, DURATION)?,
        Some(EditTarget::Clip)
    ));
    assert_eq!(project.clip, None);
    assert_eq!(project.playback_range(DURATION).duration(), DURATION);
    assert_eq!(project.captions[0].text, "ボス戦");
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("project.json");
    project.save(&path)?;
    assert_eq!(Project::load(&path)?.clip, None);

    assert!(matches!(
        history.undo(&mut project, DURATION)?,
        Some(EditTarget::Caption(0))
    ));
    assert_eq!(project.captions[0].text, "開始");
    assert_eq!(project.captions[0].speaker, "A");
    assert!(!history.can_undo());

    assert!(matches!(
        history.redo(&mut project, DURATION)?,
        Some(EditTarget::Caption(0))
    ));
    assert!(matches!(
        history.redo(&mut project, DURATION)?,
        Some(EditTarget::Clip)
    ));
    assert_eq!(project.clip, Some(TRIM));
    assert_eq!(project.captions[0].text, "ボス戦");
    assert_eq!(project.captions[0].speaker, "B");
    project.save(&path)?;
    assert_eq!(Project::load(&path)?.clip, Some(TRIM));
    assert!(!history.can_redo());
    Ok(())
}

#[test]
fn only_a_successful_new_edit_clears_redo() -> Result<()> {
    let mut project = project()?;
    let mut history = EditHistory::default();
    assert!(
        history
            .apply(&mut project, DURATION, Edit::Clip(Some(TRIM)))?
            .is_some()
    );
    assert!(history.undo(&mut project, DURATION)?.is_some());

    assert!(
        history
            .apply(&mut project, DURATION, Edit::Clip(None))?
            .is_none()
    );
    let invalid = ClipRange {
        source_start_us: 0,
        source_end_us: DURATION + 1,
    };
    assert!(
        history
            .apply(&mut project, DURATION, Edit::Clip(Some(invalid)))
            .is_err()
    );
    let no_change = Edit::Caption {
        row: 0,
        id: "opening".into(),
        speaker: "A".into(),
        text: "開始".into(),
    };
    assert!(history.apply(&mut project, DURATION, no_change)?.is_none());
    assert!(history.can_redo());
    assert_eq!(project.clip, None);
    assert!(!history.can_undo());

    assert!(
        history
            .apply(&mut project, DURATION, caption("別の字幕"))?
            .is_some()
    );
    assert!(history.can_undo());
    assert!(!history.can_redo());
    assert!(history.redo(&mut project, DURATION)?.is_none());
    assert_eq!(project.captions[0].text, "別の字幕");
    Ok(())
}

#[test]
fn undo_refuses_to_modify_a_different_caption_in_the_same_row() -> Result<()> {
    let mut project = project()?;
    let mut history = EditHistory::default();
    assert!(
        history
            .apply(&mut project, DURATION, caption("ボス戦"))?
            .is_some()
    );
    project.captions[0].id = "replacement".into();
    project.captions[0].text = "別の字幕".into();

    assert!(history.undo(&mut project, DURATION).is_err());
    assert!(history.can_undo());
    assert!(!history.can_redo());
    assert_eq!(project.captions[0].text, "別の字幕");
    Ok(())
}

#[test]
fn saved_undo_and_redo_survive_reopening_v1_and_v2() -> Result<()> {
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("project.json");
    let mut project = project()?;
    let mut history = EditHistory::default();
    history.apply(&mut project, DURATION, caption("ボス戦"))?;
    history.apply(&mut project, DURATION, Edit::Clip(Some(TRIM)))?;
    history.undo(&mut project, DURATION)?;

    project.save_with_history(&path, &history)?;
    let saved: Value = serde_json::from_slice(&fs::read(&path)?)?;
    assert_eq!(saved["version"], 1);
    assert_eq!(saved["video"], "recording.mp4");

    let mut reopened = Project::load(&path)?;
    let mut history = reopened.take_edit_history(DURATION)?;
    assert_eq!(reopened.clip, None);
    assert_eq!(reopened.captions[0].text, "ボス戦");
    assert!(history.can_undo() && history.can_redo());
    assert!(matches!(
        history.redo(&mut reopened, DURATION)?,
        Some(EditTarget::Clip)
    ));
    assert_eq!(reopened.clip, Some(TRIM));
    reopened.save_with_history(&path, &history)?;

    let saved: Value = serde_json::from_slice(&fs::read(&path)?)?;
    assert_eq!(saved["version"], 2);
    let mut reopened = Project::load(&path)?;
    let mut history = reopened.take_edit_history(DURATION)?;
    assert!(matches!(
        history.undo(&mut reopened, DURATION)?,
        Some(EditTarget::Clip)
    ));
    assert!(matches!(
        history.undo(&mut reopened, DURATION)?,
        Some(EditTarget::Caption(0))
    ));
    assert_eq!(reopened.clip, None);
    assert_eq!(reopened.captions[0].text, "開始");
    assert_eq!(reopened.captions[0].speaker, "A");
    assert!(!history.can_undo());
    Ok(())
}

#[test]
fn clip_and_caption_basis_undo_and_redo_together_after_reopening() -> Result<()> {
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("project.json");
    let mut project = project()?;
    let mut history = EditHistory::default();
    // An AI selection changes both values, then a manual trim leaves the basis alone.
    assert!(matches!(
        history.apply(
            &mut project,
            DURATION,
            Edit::ClipWithCaptionBasis {
                range: Some(TRIM),
                captions_use_source_time: true,
            },
        )?,
        Some(EditTarget::Clip)
    ));
    let manual = ClipRange {
        source_start_us: TRIM.source_start_us + SECOND,
        ..TRIM
    };
    history.apply(&mut project, DURATION, Edit::Clip(Some(manual)))?;
    project.save_with_history(&path, &history)?;

    let mut reopened = Project::load(&path)?;
    let mut history = reopened.take_edit_history(DURATION)?;
    assert_eq!(reopened.clip, Some(manual));
    assert!(reopened.captions_use_source_time);
    assert!(history.undo(&mut reopened, DURATION)?.is_some());
    assert_eq!(reopened.clip, Some(TRIM));
    assert!(reopened.captions_use_source_time);
    assert!(history.undo(&mut reopened, DURATION)?.is_some());
    assert_eq!(reopened.clip, None);
    assert!(!reopened.captions_use_source_time);
    reopened.save_with_history(&path, &history)?;

    let mut reopened = Project::load(&path)?;
    let mut history = reopened.take_edit_history(DURATION)?;
    assert!(history.redo(&mut reopened, DURATION)?.is_some());
    assert_eq!(reopened.clip, Some(TRIM));
    assert!(reopened.captions_use_source_time);
    assert!(history.redo(&mut reopened, DURATION)?.is_some());
    assert_eq!(reopened.clip, Some(manual));
    assert!(reopened.captions_use_source_time);
    assert!(!history.can_redo());
    Ok(())
}

#[test]
fn caption_basis_only_edit_is_replayable_and_invalid_clip_leaves_it_unchanged() -> Result<()> {
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("project.json");
    let mut project = project()?;
    let mut history = EditHistory::default();
    assert!(
        history
            .apply(
                &mut project,
                DURATION,
                Edit::ClipWithCaptionBasis {
                    range: None,
                    captions_use_source_time: true,
                },
            )?
            .is_some()
    );
    project.save_with_history(&path, &history)?;
    let mut reopened = Project::load(&path)?;
    let mut history = reopened.take_edit_history(DURATION)?;
    assert!(history.undo(&mut reopened, DURATION)?.is_some());
    assert!(!reopened.captions_use_source_time);

    let invalid = ClipRange {
        source_end_us: DURATION + 1,
        ..TRIM
    };
    assert!(
        history
            .apply(
                &mut reopened,
                DURATION,
                Edit::ClipWithCaptionBasis {
                    range: Some(invalid),
                    captions_use_source_time: true,
                },
            )
            .is_err()
    );
    assert_eq!(reopened.clip, None);
    assert!(!reopened.captions_use_source_time);
    assert!(history.can_redo());
    Ok(())
}

#[test]
fn absent_or_invalid_archive_does_not_change_the_loaded_project() -> Result<()> {
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("project.json");
    let mut project = project()?;
    project.save(&path)?;
    let mut legacy = Project::load(&path)?;
    let empty = legacy.take_edit_history(DURATION)?;
    assert!(!empty.can_undo() && !empty.can_redo());
    legacy.set_clip(TRIM, DURATION)?;
    legacy.save(&path)?;
    let empty = Project::load(&path)?.take_edit_history(DURATION)?;
    assert!(!empty.can_undo() && !empty.can_redo());

    let doc: Value = serde_json::from_slice(&fs::read(&path)?)?;
    let same = json!({"kind": "caption", "value": {
        "row": 0, "id": "opening", "speaker": "A", "text": "開始"
    }});
    for archive in [
        json!("x".repeat(2 * 1024 * 1024)),
        json!({"version": 99, "undo": [], "redo": []}),
        json!({"version": 1, "undo": [{"kind": "unsupported"}], "redo": []}),
        json!({"version": 1, "undo": [same.clone()], "redo": []}),
        json!({"version": 1, "undo": [], "redo": vec![same.clone(); 129]}),
        json!({"version": 1, "undo": [{"kind": "caption", "value": {
            "row": 0, "id": "other", "speaker": "B", "text": "別の字幕"
        }}], "redo": []}),
        json!({"version": 1, "undo": [], "redo": [{"kind": "clip", "value": {
            "source_start_us": 0, "source_end_us": DURATION + 1
        }}]}),
        json!({"version": 1, "undo": [], "redo": [{"kind": "clip_with_caption_basis", "value": {
            "range": TRIM, "captions_use_source_time": false
        }}]}),
        json!({"version": 1, "undo": [], "redo": [{"kind": "clip_with_caption_basis", "value": {
            "captions_use_source_time": true
        }}]}),
        json!({"version": 1, "undo": [], "redo": [{"kind": "clip_with_caption_basis", "value": {
            "range": {"source_start_us": 0, "source_end_us": DURATION + 1},
            "captions_use_source_time": true
        }}]}),
    ] {
        let mut doc = doc.clone();
        doc["edit_history"] = archive;
        fs::write(&path, serde_json::to_vec(&doc)?)?;
        let mut reopened = Project::load(&path)?;
        assert!(reopened.take_edit_history(DURATION).is_err());
        assert_eq!(reopened.captions[0].text, "開始");
        assert_eq!(reopened.clip, Some(TRIM));
        assert!(!reopened.captions_use_source_time);
    }
    Ok(())
}

#[test]
fn saves_only_the_contiguous_recent_suffix() -> Result<()> {
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("project.json");
    let mut project = project()?;
    let mut history = EditHistory::default();
    for n in 1..=130 {
        history.apply(&mut project, DURATION, caption(&n.to_string()))?;
    }
    project.save_with_history(&path, &history)?;
    let mut reopened = Project::load(&path)?;
    let mut restored = reopened.take_edit_history(DURATION)?;
    for _ in 0..128 {
        assert!(restored.undo(&mut reopened, DURATION)?.is_some());
    }
    assert!(!restored.can_undo());
    assert_eq!(reopened.captions[0].text, "2");

    let long_caption = "x".repeat(512 * 1024);
    history.apply(&mut project, DURATION, caption(&long_caption))?;
    history.apply(&mut project, DURATION, caption("短い字幕"))?;
    history.apply(&mut project, DURATION, caption("最終変更"))?;
    project.save_with_history(&path, &history)?;
    let mut reopened = Project::load(&path)?;
    let mut restored = reopened.take_edit_history(DURATION)?;
    restored.undo(&mut reopened, DURATION)?;
    assert_eq!(reopened.captions[0].text, "短い字幕");
    assert!(!restored.can_undo());
    Ok(())
}

#[derive(serde::Deserialize)]
struct ItfInt {
    #[serde(rename = "#bigint")]
    decimal: String,
}

impl ItfInt {
    fn value(&self) -> Result<i32> {
        Ok(self.decimal.parse()?)
    }
}

#[derive(serde::Deserialize)]
struct HistoryTrace {
    states: Vec<HistoryTraceStep>,
}

#[derive(serde::Deserialize)]
struct HistoryTraceStep {
    state: HistoryTraceState,
}

#[derive(serde::Deserialize)]
struct HistoryTraceState {
    command: HistoryTraceCommand,
    result: String,
    before: HistoryTraceTimeline,
    after: HistoryTraceTimeline,
}

#[derive(serde::Deserialize)]
struct HistoryTraceCommand {
    kind: String,
    value: ItfInt,
}

#[derive(serde::Deserialize)]
struct HistoryTraceTimeline {
    cursor: ItfInt,
    snapshots: Vec<HistoryTraceSnapshot>,
}

#[derive(serde::Deserialize)]
struct HistoryTraceSnapshot {
    caption: ItfInt,
    clip: ItfInt,
}

#[derive(Debug, PartialEq, Eq)]
enum ObservedTarget {
    Caption(usize),
    Clip,
    SubtitleStyle,
}

impl From<EditTarget> for ObservedTarget {
    fn from(target: EditTarget) -> Self {
        match target {
            EditTarget::Caption(row) => Self::Caption(row),
            EditTarget::Clip => Self::Clip,
            EditTarget::SubtitleStyle => Self::SubtitleStyle,
        }
    }
}

fn trace_caption(value: i32) -> Result<(&'static str, &'static str)> {
    match value {
        0 => Ok(("A", "開始")),
        1 => Ok(("B", "ボス戦")),
        _ => anyhow::bail!("unknown model caption {value}"),
    }
}

fn trace_clip(value: i32) -> Result<Option<ClipRange>> {
    match value {
        0 => Ok(None),
        1 => Ok(Some(TRIM)),
        -1 => Ok(Some(ClipRange {
            source_start_us: DURATION,
            source_end_us: DURATION,
        })),
        _ => anyhow::bail!("unknown model clip {value}"),
    }
}

#[test]
fn replay_quint_edit_history_trace() -> Result<()> {
    replay_history_trace(include_str!(
        "../../verification/quint/traces/edit_history.itf.json"
    ))
}

#[test]
fn replay_quint_full_edit_history_trace() -> Result<()> {
    replay_history_trace(include_str!(
        "../../verification/quint/traces/edit_history_full.itf.json"
    ))
}

fn replay_history_trace(json: &str) -> Result<()> {
    let trace: HistoryTrace = serde_json::from_str(json)?;
    assert!(trace.states.len() > 1);
    let mut project = project()?;
    let mut history = EditHistory::default();
    for (index, step) in trace.states.into_iter().enumerate() {
        let state = step.state;
        let value = state.command.value.value()?;
        let action = state.command.kind.as_str();
        let message = format!("ITF state {index} after {action}({value})");
        let actual = match action {
            "init" if index == 0 => Ok(None),
            "caption" => {
                let (speaker, text) = trace_caption(value)?;
                history.apply(
                    &mut project,
                    DURATION,
                    Edit::Caption {
                        row: 0,
                        id: "opening".into(),
                        speaker: speaker.into(),
                        text: text.into(),
                    },
                )
            }
            "clip" => history.apply(&mut project, DURATION, Edit::Clip(trace_clip(value)?)),
            "undo" => history.undo(&mut project, DURATION),
            "redo" => history.redo(&mut project, DURATION),
            _ => anyhow::bail!("unknown operation at {message}"),
        };
        let result = match &actual {
            Ok(Some(_)) => "changed",
            Ok(None) => "unchanged",
            Err(_) => "error",
        };
        assert_eq!(result, state.result, "{message}");
        let cursor = usize::try_from(state.after.cursor.value()?)?;
        let snapshot = &state.after.snapshots[cursor];
        if let Ok(Some(target)) = actual {
            let previous = &state.before.snapshots[usize::try_from(state.before.cursor.value()?)?];
            let caption_changed = previous.caption.value()? != snapshot.caption.value()?;
            let clip_changed = previous.clip.value()? != snapshot.clip.value()?;
            let expected = match (caption_changed, clip_changed) {
                (true, false) => ObservedTarget::Caption(0),
                (false, true) => ObservedTarget::Clip,
                _ => anyhow::bail!("expected exactly one changed field at {message}"),
            };
            assert_eq!(ObservedTarget::from(target), expected, "{message}");
        }
        let (speaker, text) = trace_caption(snapshot.caption.value()?)?;
        assert_eq!(project.captions[0].speaker, speaker, "{message}");
        assert_eq!(project.captions[0].text, text, "{message}");
        assert_eq!(
            project.clip,
            trace_clip(snapshot.clip.value()?)?,
            "{message}"
        );
        assert_eq!(history.can_undo(), cursor > 0, "{message}");
        assert_eq!(
            history.can_redo(),
            cursor + 1 < state.after.snapshots.len(),
            "{message}"
        );
    }
    Ok(())
}

#[derive(Clone, Debug)]
enum HistoryOperation {
    Caption(String),
    Clip(Option<ClipRange>),
    Undo,
    Redo,
}

impl HistoryOperation {
    fn apply(
        &self,
        project: &mut Project,
        history: &mut EditHistory,
    ) -> Result<Option<EditTarget>> {
        match self {
            Self::Caption(text) => history.apply(project, DURATION, caption(text)),
            Self::Clip(range) => history.apply(project, DURATION, Edit::Clip(*range)),
            Self::Undo => history.undo(project, DURATION),
            Self::Redo => history.redo(project, DURATION),
        }
    }
}

fn history_operation() -> impl proptest::strategy::Strategy<Value = HistoryOperation> {
    use proptest::prelude::*;
    prop_oneof![
        "(?s:.{0,24})".prop_map(HistoryOperation::Caption),
        Just(HistoryOperation::Clip(None)),
        Just(HistoryOperation::Clip(Some(TRIM))),
        Just(HistoryOperation::Undo),
        Just(HistoryOperation::Redo),
    ]
}

fn observe_history<'a>(
    project: &'a Project,
    history: &EditHistory,
) -> (&'a str, &'a str, Option<ClipRange>, bool, bool) {
    (
        &project.captions[0].speaker,
        &project.captions[0].text,
        project.clip,
        history.can_undo(),
        history.can_redo(),
    )
}

proptest::proptest! {
    #[test]
    fn generated_history_behaves_the_same_after_reopening(
        prefix in proptest::collection::vec(history_operation(), 1..24),
        suffix in proptest::collection::vec(history_operation(), 1..24),
    ) {
        use proptest::{prop_assert_eq, test_runner::TestCaseError};

        let test_error = |error: anyhow::Error| TestCaseError::fail(format!("{error:#}"));

        let mut original = project().map_err(test_error)?;
        let mut history = EditHistory::default();
        let mut project_to_save = project().map_err(test_error)?;
        let mut history_to_save = EditHistory::default();
        for operation in prefix {
            operation.apply(&mut original, &mut history).map_err(test_error)?;
            operation.apply(&mut project_to_save, &mut history_to_save).map_err(test_error)?;
        }
        let folder = tempfile::tempdir()?;
        let path = folder.path().join("project.json");
        project_to_save.save_with_history(&path, &history_to_save).map_err(test_error)?;
        let mut reopened = Project::load(&path).map_err(test_error)?;
        let mut restored = reopened.take_edit_history(DURATION).map_err(test_error)?;
        prop_assert_eq!(observe_history(&original, &history), observe_history(&reopened, &restored));

        // Drain the same commands after reopening, then exhaust undo and redo.
        // Either stack can contain at most one entry per generated operation.
        for operation in suffix.into_iter()
            .chain(std::iter::repeat_n(HistoryOperation::Undo, 48))
            .chain(std::iter::repeat_n(HistoryOperation::Redo, 48))
        {
            let expected = operation.apply(&mut original, &mut history).map_err(test_error)?;
            let actual = operation.apply(&mut reopened, &mut restored).map_err(test_error)?;
            prop_assert_eq!(
                actual.map(ObservedTarget::from), expected.map(ObservedTarget::from),
                "after {:?}", operation
            );
            prop_assert_eq!(
                observe_history(&original, &history), observe_history(&reopened, &restored),
                "after {:?}", operation
            );
        }
    }
}

#[test]
fn short_operation_sequences_match_an_independent_snapshot_model() -> Result<()> {
    #[derive(Clone, Copy, Debug)]
    enum Operation {
        EditCaption,
        EditClip,
        Undo,
        Redo,
        Reopen,
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Snapshot {
        text: &'static str,
        clip: Option<ClipRange>,
    }

    const OPERATIONS: [Operation; 5] = [
        Operation::EditCaption,
        Operation::EditClip,
        Operation::Undo,
        Operation::Redo,
        Operation::Reopen,
    ];
    const LENGTH: usize = 5;

    let folder = tempfile::tempdir()?;
    let path = folder.path().join("project.json");
    for mut index in 0..OPERATIONS.len().pow(LENGTH as u32) {
        let sequence: [Operation; LENGTH] = std::array::from_fn(|_| {
            let operation = OPERATIONS[index % OPERATIONS.len()];
            index /= OPERATIONS.len();
            operation
        });
        let mut project = project()?;
        let mut history = EditHistory::default();
        let mut snapshots = vec![Snapshot {
            text: "開始",
            clip: None,
        }];
        let mut cursor = 0;

        for (step, operation) in sequence.iter().enumerate() {
            let mut next = snapshots[cursor];
            let (changed, expected_change) = match operation {
                Operation::EditCaption | Operation::EditClip => {
                    let edit = match operation {
                        Operation::EditCaption => {
                            next.text = "ボス戦";
                            caption(next.text)
                        }
                        Operation::EditClip => {
                            next.clip = Some(TRIM);
                            Edit::Clip(next.clip)
                        }
                        _ => unreachable!(),
                    };
                    let expected_change = next != snapshots[cursor];
                    if expected_change {
                        snapshots.truncate(cursor + 1);
                        snapshots.push(next);
                        cursor += 1;
                    }
                    (
                        history.apply(&mut project, DURATION, edit)?.is_some(),
                        expected_change,
                    )
                }
                Operation::Undo => {
                    let expected_change = cursor > 0;
                    cursor = cursor.saturating_sub(1);
                    (
                        history.undo(&mut project, DURATION)?.is_some(),
                        expected_change,
                    )
                }
                Operation::Redo => {
                    let expected_change = cursor + 1 < snapshots.len();
                    cursor += usize::from(expected_change);
                    (
                        history.redo(&mut project, DURATION)?.is_some(),
                        expected_change,
                    )
                }
                Operation::Reopen => {
                    project.save_with_history(&path, &history)?;
                    project = Project::load(&path)?;
                    history = project.take_edit_history(DURATION)?;
                    (false, false)
                }
            };
            assert_eq!(
                (
                    changed,
                    project.captions[0].text.as_str(),
                    project.clip,
                    history.can_undo(),
                    history.can_redo(),
                ),
                (
                    expected_change,
                    snapshots[cursor].text,
                    snapshots[cursor].clip,
                    cursor > 0,
                    cursor + 1 < snapshots.len(),
                ),
                "sequence {:?}",
                &sequence[..=step],
            );
        }
    }
    Ok(())
}
