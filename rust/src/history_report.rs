//! Exercise the editor's UI callbacks and record visible Undo/Redo outcomes.

use super::{Editor, EditorWindow, Loaded, Project, TICK, capture_window};
use anyhow::{Result, ensure};
use serde::Serialize;
use slint::{ComponentHandle, Model, Timer, TimerMode};
use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, Instant},
};

const CAPTION: &str = "ボス戦";
const OPEN_SAMPLE: &str = "合成プロジェクトを開く";
const ENTER_CLIP_RANGE: &str = "開始に「0」、終了に「9」を入力して「区間を適用」を押す";
const EDIT_FIRST_CAPTION: &str = "字幕一覧の先頭行を選び、本文を「ボス戦」に書き換える";
const UNDO: &str = "ツールバーの「元に戻す」を押す";
const ORIGINAL_RANGE: &str = "0.000000 – 60.000000 秒";
const EDITED_RANGE: &str = "0.000000 – 9.000000 秒";
const ENABLED: &str = "有効";
const DISABLED: &str = "無効";
const SCENARIO_NAMES: [&str; 2] = [
    "Undo の後の新しい編集で通常の Redo の候補を消す",
    "保存後に開き直したプロジェクトで Undo と Redo を使う",
];

#[derive(Serialize)]
struct Report {
    scenarios: Vec<Scenario>,
}

#[derive(Serialize)]
struct Scenario {
    name: &'static str,
    steps: Vec<Step>,
}

#[derive(Serialize)]
struct Step {
    action: &'static str,
    operations: &'static [&'static str],
    checks: Vec<Check>,
    screenshot: String,
}

#[derive(Serialize)]
struct Check {
    name: &'static str,
    expected: &'static str,
    actual: String,
}

fn check(name: &'static str, expected: &'static str, actual: impl Into<String>) -> Check {
    Check {
        name,
        expected,
        actual: actual.into(),
    }
}

fn button(enabled: bool) -> &'static str {
    if enabled { ENABLED } else { DISABLED }
}

fn clip(ui: &EditorWindow) -> String {
    format!("{} – {} 秒", ui.get_clip_start(), ui.get_clip_end())
}

fn caption(ui: &EditorWindow) -> String {
    ui.get_captions()
        .row_data(0)
        .map_or_else(String::new, |row| row.body.to_string())
}

fn edit_caption(ui: &EditorWindow) {
    ui.invoke_select_caption(0);
    ui.set_caption_text(CAPTION.into());
    ui.invoke_update_caption();
}

fn edit_clip(ui: &EditorWindow) {
    ui.set_clip_start("0".into());
    ui.set_clip_end("9".into());
    ui.invoke_update_clip();
}

fn open_project(
    ui: &EditorWindow,
    editor: &RefCell<Editor>,
    source: &Path,
    destination: &Path,
) -> Result<()> {
    let loaded = Loaded::open(Project::load(source)?, destination.to_path_buf())?;
    editor.borrow_mut().install(ui, loaded);
    ui.invoke_select_caption(0);
    Ok(())
}

fn perform(
    ui: &EditorWindow,
    editor: &RefCell<Editor>,
    source: &Path,
    destination: &Path,
    scenario: usize,
    step: usize,
) -> Result<Step> {
    let (action, operations, checks): (_, &[_], _) = match (scenario, step) {
        (0, 0) => {
            open_project(ui, editor, source, destination)?;
            edit_clip(ui);
            (
                "素材区間を0〜9秒に変更する",
                &[OPEN_SAMPLE, ENTER_CLIP_RANGE],
                vec![check("素材区間", EDITED_RANGE, clip(ui))],
            )
        }
        (0, 1) => {
            ui.invoke_undo_edit();
            (
                "区間変更を元に戻す",
                &[UNDO],
                vec![
                    check("素材区間", ORIGINAL_RANGE, clip(ui)),
                    check(
                        "やり直すボタン",
                        ENABLED,
                        button(ui.invoke_history_redo_enabled()),
                    ),
                ],
            )
        }
        (0, 2) => {
            edit_caption(ui);
            (
                "別の編集として字幕本文を変更する",
                &[EDIT_FIRST_CAPTION],
                vec![
                    check("字幕一覧の本文", CAPTION, caption(ui)),
                    check(
                        "やり直すボタン",
                        DISABLED,
                        button(ui.invoke_history_redo_enabled()),
                    ),
                ],
            )
        }
        (1, 0) => {
            open_project(ui, editor, source, destination)?;
            edit_caption(ui);
            edit_clip(ui);
            (
                "字幕と素材区間を順に変更する",
                &[OPEN_SAMPLE, EDIT_FIRST_CAPTION, ENTER_CLIP_RANGE],
                vec![
                    check("字幕一覧の本文", CAPTION, caption(ui)),
                    check("素材区間", EDITED_RANGE, clip(ui)),
                ],
            )
        }
        (1, 1) => {
            ui.invoke_undo_edit();
            ui.invoke_save_project();
            (
                "区間変更だけを元に戻して保存する",
                &[UNDO, "ツールバーの「保存」を押す"],
                vec![
                    check("素材区間", ORIGINAL_RANGE, clip(ui)),
                    check(
                        "保存結果",
                        "プロジェクトを保存しました",
                        ui.get_status().to_string(),
                    ),
                ],
            )
        }
        (1, 2) => {
            open_project(ui, editor, destination, destination)?;
            (
                "保存したプロジェクトを開き直す",
                &["直前に保存した fixture.json を再度開く"],
                vec![
                    check("字幕一覧の本文", CAPTION, caption(ui)),
                    check("素材区間", ORIGINAL_RANGE, clip(ui)),
                    check(
                        "元に戻すボタン",
                        ENABLED,
                        button(ui.invoke_history_undo_enabled()),
                    ),
                    check(
                        "やり直すボタン",
                        ENABLED,
                        button(ui.invoke_history_redo_enabled()),
                    ),
                ],
            )
        }
        (1, 3) => {
            ui.invoke_redo_edit();
            (
                "区間変更をやり直す",
                &["ツールバーの「やり直す」を押す"],
                vec![check("素材区間", EDITED_RANGE, clip(ui))],
            )
        }
        _ => unreachable!(),
    };
    Ok(Step {
        action,
        operations,
        checks,
        screenshot: format!("{}-{}.png", scenario + 1, step + 1),
    })
}

pub(super) fn run(
    ui: &EditorWindow,
    editor: &Rc<RefCell<Editor>>,
    source: PathBuf,
    output: PathBuf,
) -> Result<()> {
    fs::create_dir_all(&output)?;
    let workspace = tempfile::tempdir()?;
    let destination = workspace.path().join("fixture.json");
    let mut report = Report {
        scenarios: SCENARIO_NAMES
            .map(|name| Scenario {
                name,
                steps: Vec::new(),
            })
            .into(),
    };
    let mut scenario = 0;
    let mut step = 0;
    let mut pending: Option<(Step, Instant)> = None;
    let error = Rc::new(RefCell::new(None));
    let deadline = Instant::now() + Duration::from_secs(40);
    let timer = Timer::default();
    {
        let window = ui.as_weak();
        let editor = editor.clone();
        let error = error.clone();
        timer.start(TimerMode::Repeated, TICK, move || {
            let Some(ui) = window.upgrade() else { return };
            let result = (|| -> Result<bool> {
                ensure!(
                    Instant::now() < deadline,
                    "履歴の画面検査がタイムアウトしました"
                );
                editor.borrow_mut().tick(&ui)?;
                if let Some((capture, ready_at)) = pending.as_ref() {
                    let failed = capture
                        .checks
                        .iter()
                        .any(|check| check.expected != check.actual);
                    if Instant::now() < *ready_at
                        || (!failed && editor.borrow().last_frame.is_none())
                    {
                        return Ok(false);
                    }
                    capture_window(&ui, &output.join(&capture.screenshot))?;
                    let capture = pending.take().unwrap().0;
                    report.scenarios[scenario].steps.push(capture);
                    step += 1;
                    if failed || step == [3, 4][scenario] {
                        scenario += 1;
                        step = 0;
                    }
                    if scenario == report.scenarios.len() {
                        let file = fs::File::create(output.join("report.json"))?;
                        serde_json::to_writer_pretty(file, &report)?;
                        ensure!(
                            report
                                .scenarios
                                .iter()
                                .flat_map(|scenario| &scenario.steps)
                                .flat_map(|step| &step.checks)
                                .all(|check| check.expected == check.actual),
                            "履歴の画面検査に失敗しました"
                        );
                        return Ok(true);
                    }
                    return Ok(false);
                }
                let capture = perform(&ui, &editor, &source, &destination, scenario, step)
                    .unwrap_or_else(|error| {
                        eprintln!(
                            "{} の操作 {}: {error:#}",
                            SCENARIO_NAMES[scenario],
                            step + 1
                        );
                        Step {
                            action: "プロジェクトを開く",
                            operations: &["シナリオで使うプロジェクトを読み込む"],
                            checks: vec![check("プロジェクトの読み込み", "成功", "失敗")],
                            screenshot: format!("{}-{}.png", scenario + 1, step + 1),
                        }
                    });
                pending = Some((capture, Instant::now() + Duration::from_millis(180)));
                Ok(false)
            })();
            match result {
                Ok(false) => {}
                Ok(true) => {
                    let _ = slint::quit_event_loop();
                }
                Err(failure) => {
                    *error.borrow_mut() = Some(failure);
                    let _ = slint::quit_event_loop();
                }
            }
        });
    }
    ui.run().map_err(anyhow::Error::msg)?;
    timer.stop();
    if let Some(failure) = error.borrow_mut().take() {
        return Err(failure);
    }
    Ok(())
}
