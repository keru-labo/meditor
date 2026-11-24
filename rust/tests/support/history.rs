use anyhow::Result;
use meditor_playback::{
    history::Edit,
    project::{Caption, ClipRange, Project, SECOND},
};

pub const DURATION: i64 = 60 * SECOND;
pub const TRIM: ClipRange = ClipRange {
    source_start_us: 9 * SECOND,
    source_end_us: 34 * SECOND,
};

pub fn project() -> Result<Project> {
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

pub fn caption(text: &str) -> Edit {
    Edit::Caption {
        row: 0,
        id: "opening".into(),
        speaker: "B".into(),
        text: text.into(),
    }
}
