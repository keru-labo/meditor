//! Runs one compiled Gherkin scenario against the editor's history and project APIs.

use anyhow::{Result, bail, ensure};
use meditor_playback::{
    history::{Edit, EditHistory, EditTarget},
    project::Project,
};
use std::io;

#[path = "../tests/support/history.rs"]
mod fixture;

use fixture::{DURATION, TRIM, caption, project};

fn main() -> Result<()> {
    let steps: Vec<String> = serde_json::from_reader(io::stdin().lock())?;
    ensure!(!steps.is_empty(), "scenario has no steps");

    let directory = tempfile::tempdir()?;
    let path = directory.path().join("project.json");
    let mut project = project()?;
    let mut history = EditHistory::default();

    for step in steps {
        match step.as_str() {
            "制作者が以前のクリップ変更を元に戻し、Redo ができる" => {
                history.apply(&mut project, DURATION, Edit::Clip(Some(TRIM)))?;
                ensure!(
                    matches!(
                        history.undo(&mut project, DURATION)?,
                        Some(EditTarget::Clip)
                    ) && project.clip.is_none()
                        && history.can_redo(),
                    "the clip edit must be undone and redoable"
                );
            }
            "制作者が代わりに字幕を修正し、その変更を適用する" => {
                history.apply(&mut project, DURATION, caption("別の字幕"))?;
            }
            "字幕の修正が現在の編集状態に反映される" => {
                ensure!(
                    project.captions[0].text == "別の字幕",
                    "caption edit was lost"
                );
            }
            "通常の Redo は利用できなくなる" => {
                ensure!(
                    !history.can_redo(),
                    "redo remains available after a new edit"
                );
                ensure!(
                    history.redo(&mut project, DURATION)?.is_none() && project.clip.is_none(),
                    "redo replayed the discarded change"
                );
            }
            "制作者が字幕本文を「ボス戦」に編集し、続けてクリップの区間を変更した" =>
            {
                history.apply(&mut project, DURATION, caption("ボス戦"))?;
                history.apply(&mut project, DURATION, Edit::Clip(Some(TRIM)))?;
            }
            "区間変更だけを元に戻してプロジェクトを保存した" => {
                ensure!(
                    matches!(
                        history.undo(&mut project, DURATION)?,
                        Some(EditTarget::Clip)
                    ),
                    "undo did not reverse the clip edit"
                );
                project.save_with_history(&path, &history)?;
            }
            "制作者が保存したプロジェクトを開き直す" => {
                project = Project::load(&path)?;
                history = project.take_edit_history(DURATION)?;
            }
            "字幕は「ボス戦」で、クリップの区間は Undo 後の値である" => {
                ensure!(
                    project.captions[0].text == "ボス戦" && project.clip.is_none(),
                    "reopened project has the wrong caption or clip"
                );
            }
            "「元に戻す」と「やり直す」の両方を使える" => {
                ensure!(
                    history.can_undo() && history.can_redo(),
                    "history stacks were lost"
                );
            }
            "制作者が「やり直す」を押す" => {
                ensure!(
                    matches!(
                        history.redo(&mut project, DURATION)?,
                        Some(EditTarget::Clip)
                    ),
                    "redo did not replay the clip edit"
                );
            }
            "保存前に元に戻した区間変更が適用される" => {
                ensure!(project.clip == Some(TRIM), "incorrect clip after redo");
            }
            _ => bail!("undefined Gherkin step: {step}"),
        }
    }

    Ok(())
}
