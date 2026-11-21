use anyhow::Result;
use meditor_playback::{
    history::{Edit, EditHistory, EditTarget},
    project::{Caption, ClipRange, Project, SECOND},
};

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
