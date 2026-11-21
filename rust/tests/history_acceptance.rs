use anyhow::Result;
use meditor_playback::{
    history::{Edit, EditHistory, EditTarget},
    project::{Caption, ClipRange, Project, SECOND},
};
use serde_json::{Value, json};
use std::fs;

const DURATION: i64 = 60 * SECOND;
const TRIM: ClipRange = ClipRange {
    source_start_us: 9 * SECOND,
    source_end_us: 34 * SECOND,
};

fn project() -> Result<Project> {
    let mut project = Project::new("recording.mp4".into(), Vec::new())?;
    project.captions.push(Caption {
        id: "opening".into(),
        start_us: SECOND,
        end_us: 2 * SECOND,
        speaker: "A".into(),
        text: "開始".into(),
    });
    Ok(project)
}

fn caption(text: &str) -> Edit {
    Edit::Caption {
        row: 0,
        id: "opening".into(),
        speaker: "B".into(),
        text: text.into(),
    }
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
