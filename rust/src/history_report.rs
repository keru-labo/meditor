//! Dispatch pointer and keyboard events to the editor and record visible BDD outcomes.

use super::editor_dialogs::DialogAnswer;
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

#[path = "editor_controls_report.rs"]
mod editor_controls;
#[path = "ui_report_input.rs"]
mod input;
#[path = "history_report_spec.rs"]
mod spec;

const CAPTION: &str = "ボス戦";
const OPEN_SAMPLE: &str = "合成プロジェクトを開く";
const ENTER_CLIP_RANGE: &str = "開始に「0」、終了に「9」を入力して「区間を適用」を押す";
const EDIT_FIRST_CAPTION: &str = "字幕一覧の先頭行を選び、本文を「ボス戦」に書き換える";
const UNDO: &str = "ツールバーの「元に戻す」を押す";
const ORIGINAL_RANGE: &str = "0.000000 – 60.000000 秒";
const EDITED_RANGE: &str = "0.000000 – 9.000000 秒";
const ENABLED: &str = "有効";
const DISABLED: &str = "無効";
#[derive(Serialize)]
struct Report {
    input_method: &'static str,
    scenarios: Vec<Scenario>,
}

#[derive(Serialize)]
struct Scenario {
    evidence_id: &'static str,
    name: String,
    steps: Vec<Step>,
}

#[derive(Serialize)]
struct Step {
    action: &'static str,
    operations: &'static [&'static str],
    rationale: &'static str,
    checks: Vec<Check>,
    screenshot: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    spec: Vec<spec::SpecStep>,
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

fn edit_caption(ui: &EditorWindow) -> Result<()> {
    input::click(ui, "caption-first")?;
    input::replace(ui, "caption-body", CAPTION)
}

fn edit_clip(ui: &EditorWindow) -> Result<()> {
    input::replace(ui, "clip-start", "0")?;
    input::replace(ui, "clip-end", "9")?;
    input::click(ui, "clip-apply")
}

fn open_project(
    ui: &EditorWindow,
    editor: &RefCell<Editor>,
    source: &Path,
    destination: &Path,
) -> Result<()> {
    let loaded = Loaded::open(Project::load(source)?, destination.to_path_buf())?;
    editor.borrow_mut().install(ui, loaded);
    Ok(())
}

fn reopen_project(ui: &EditorWindow, editor: &RefCell<Editor>, path: &Path) -> Result<()> {
    let result = click_with_files(
        ui,
        editor,
        "open",
        [DialogAnswer::File(Some(path.to_owned()))],
    );
    result?;
    ensure!(
        ui.get_status() == "音声 3 トラック",
        "保存したプロジェクトを開けませんでした: {}",
        ui.get_status()
    );
    Ok(())
}

fn click_with_files(
    ui: &EditorWindow,
    editor: &RefCell<Editor>,
    target: &str,
    answers: impl IntoIterator<Item = DialogAnswer>,
) -> Result<()> {
    editor.borrow_mut().dialogs.supply(answers);
    let clicked = input::click(ui, target);
    let consumed = editor.borrow().dialogs.consumed();
    editor.borrow_mut().dialogs.supply([]);
    clicked?;
    ensure!(consumed, "ファイル選択まで実行されませんでした: {target}");
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
    let (action, operations, rationale, checks): (_, &[_], _, _) = match (scenario, step) {
        (0, 0) => {
            open_project(ui, editor, source, destination)?;
            edit_clip(ui)?;
            (
                "素材区間を0〜9秒に変更する",
                &[OPEN_SAMPLE, ENTER_CLIP_RANGE],
                "入力した 0〜9 秒が素材区間に反映されれば、続く Undo で取り消す変更が作成されている。",
                vec![check("素材区間", EDITED_RANGE, clip(ui))],
            )
        }
        (0, 1) => {
            input::click(ui, "undo")?;
            (
                "区間変更を元に戻す",
                &[UNDO],
                "区間が元の 0〜60 秒に戻り、Redo が有効なら、取り消した区間変更をまだ再適用できる。",
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
            edit_caption(ui)?;
            (
                "別の編集として字幕本文を変更する",
                &[EDIT_FIRST_CAPTION],
                "字幕に「ボス戦」が反映され、Redo が無効なら、新しい字幕編集を適用した時点で以前の区間変更の Redo 候補が消えている。",
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
            edit_caption(ui)?;
            edit_clip(ui)?;
            (
                "字幕と素材区間を順に変更する",
                &[OPEN_SAMPLE, EDIT_FIRST_CAPTION, ENTER_CLIP_RANGE],
                "字幕が「ボス戦」で区間が 0〜9 秒なら、字幕編集と区間変更の両方が反映されている。",
                vec![
                    check("字幕一覧の本文", CAPTION, caption(ui)),
                    check("素材区間", EDITED_RANGE, clip(ui)),
                ],
            )
        }
        (1, 1) => {
            input::click(ui, "undo")?;
            input::click(ui, "save")?;
            (
                "区間変更だけを元に戻して保存する",
                &[UNDO, "ツールバーの「保存」を押す"],
                "区間だけが元の 0〜60 秒に戻り、保存完了が表示されれば、その状態で保存できた。",
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
            reopen_project(ui, editor, destination)?;
            (
                "保存したプロジェクトを開き直す",
                &["直前に保存した fixture.json を再度開く"],
                "字幕が「ボス戦」のままで区間が 0〜60 秒に戻っており、Undo と Redo の両方が有効なら、編集内容と両方向の履歴を復元できている。",
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
            input::click(ui, "redo")?;
            (
                "区間変更をやり直す",
                &["ツールバーの「やり直す」を押す"],
                "区間が再び 0〜9 秒になれば、保存前に取り消した区間変更を開き直したあとに再適用できている。",
                vec![check("素材区間", EDITED_RANGE, clip(ui))],
            )
        }
        (2 | 3 | 5..=8, 0) => {
            let (start, expected, length) = if scenario < 5 {
                (
                    30_000_000,
                    "30.000000 – 40.000000 秒",
                    "00:00.000  /  00:10.000",
                )
            } else {
                (
                    32_000_000,
                    "32.000000 – 40.000000 秒",
                    "00:00.000  /  00:08.000",
                )
            };
            let range = super::ClipRange {
                source_start_us: start,
                source_end_us: 40_000_000,
            };
            let mut project = Project::load(source)?;
            project.set_clip(range, 60_000_000)?;
            let loaded = Loaded::open(project, destination.to_path_buf())?;
            editor.borrow_mut().install(ui, loaded);
            (
                "素材区間を設定済みのプロジェクトを開く",
                &["素材区間を設定した合成プロジェクトを開く"],
                "素材の開始・終了と先頭の再生位置、区間の長さで、変更前の状態を確認する。",
                clip_timing(ui, expected, length),
            )
        }
        (2, 1) => {
            input::replace(ui, "clip-start", "32")?;
            input::click(ui, "clip-apply")?;
            (
                "クリップの開始を32秒に変更する",
                &["開始に「32」を入力して「区間を適用」を押す"],
                "素材区間の開始だけが32秒に移動し、タイムラインの長さが8秒になる。",
                clip_timing(ui, "32.000000 – 40.000000 秒", "00:00.000  /  00:08.000"),
            )
        }
        (2, 2) => {
            input::click(ui, "save")?;
            let saved = ui.get_status();
            reopen_project(ui, editor, destination)?;
            let mut checks = clip_timing(ui, "32.000000 – 40.000000 秒", "00:00.000  /  00:08.000");
            checks.push(check("保存結果", "プロジェクトを保存しました", saved));
            (
                "区間の変更を保存して開き直す",
                &["「保存」を押す", "保存したプロジェクトを開き直す"],
                "保存完了の表示が出て、開き直しても32〜40秒の区間と8秒の長さが維持される。",
                checks,
            )
        }
        (3, 1) => {
            input::replace(ui, "clip-end", "38")?;
            input::click(ui, "clip-apply")?;
            (
                "クリップの終了を38秒に変更する",
                &["終了に「38」を入力して「区間を適用」を押す"],
                "素材区間の開始は30秒のまま、終了が38秒になり、タイムラインの長さが8秒になる。",
                clip_timing(ui, "30.000000 – 38.000000 秒", "00:00.000  /  00:08.000"),
            )
        }
        (4, 0) => {
            open_project(ui, editor, source, destination)?;
            (
                "既定の字幕をプレビューする",
                &[OPEN_SAMPLE],
                "字幕が表示され、文字色と縁取りが適用前の値になっている。",
                subtitle_appearance(ui, "#FFFFFF", "#101010", "2"),
            )
        }
        (4, 1) => {
            input::click(ui, "subtitle-style")?;
            input::replace(ui, "style-fill", "#FFFF00")?;
            input::replace(ui, "style-outline", "#0044FF")?;
            input::replace(ui, "style-thickness", "8")?;
            input::click(ui, "style-apply")?;
            let mut checks = subtitle_appearance(ui, "#FFFF00", "#0044FF", "8");
            checks.push(check(
                "設定の適用",
                "字幕スタイルを更新しました",
                ui.get_status(),
            ));
            (
                "黄色の文字と太い青の縁取りを適用する",
                &["字幕スタイルの文字色を黄色、縁取りを青、太さを8に設定して適用する"],
                "設定が受理され、現在のプレビューに渡る文字色・縁取りの色・太さが更新される。",
                checks,
            )
        }
        (4, 2) => {
            input::click(ui, "undo")?;
            (
                "字幕スタイルを一度で元に戻す",
                &[UNDO],
                "Undo を一度行うと、プレビューの文字色・縁取りの色・太さがすべて適用前に戻る。",
                subtitle_appearance(ui, "#FFFFFF", "#101010", "2"),
            )
        }
        (5..=8, 1) => {
            let (_, control, input) = spec::INVALID_CLIP_INPUTS[scenario - 5];
            let field = if control == "開始" {
                "clip-start"
            } else {
                "clip-end"
            };
            input::replace(ui, field, input)?;
            input::click(ui, "clip-apply")?;
            let range = editor.borrow().loaded.as_ref().unwrap().range();
            let actual = format!(
                "{} – {} 秒",
                super::seconds_text(range.source_start_us),
                super::seconds_text(range.source_end_us),
            );
            let error = if input == "-1" {
                "開始時刻を確認してください"
            } else {
                "素材内の有効な開始・終了時刻を入力してください"
            };
            (
                "無効な時刻で区間を適用する",
                &["開始または終了に例の時刻を入力し、「区間を適用」を押す"],
                "入力エラーが表示され、適用済みの素材区間とタイムラインの長さが変更前のままである。",
                vec![
                    check("入力エラーの表示", error, ui.get_status()),
                    check("適用されている素材区間", "32.000000 – 40.000000 秒", actual),
                    check(
                        "再生位置 ／ 区間の長さ",
                        "00:00.000  /  00:08.000",
                        ui.get_clock_label(),
                    ),
                ],
            )
        }
        _ => unreachable!(),
    };
    Ok(Step {
        action,
        operations,
        rationale,
        checks,
        screenshot: format!("{}-{}.png", scenario + 1, step + 1),
        spec: Vec::new(),
    })
}

fn clip_timing(ui: &EditorWindow, range: &'static str, clock: &'static str) -> Vec<Check> {
    vec![
        check("素材区間", range, clip(ui)),
        check("再生位置 ／ 区間の長さ", clock, ui.get_clock_label()),
    ]
}

fn subtitle_appearance(
    ui: &EditorWindow,
    text: &'static str,
    outline: &'static str,
    width: &'static str,
) -> Vec<Check> {
    fn hex(color: slint::Color) -> String {
        format!(
            "#{:02X}{:02X}{:02X}",
            color.red(),
            color.green(),
            color.blue()
        )
    }
    let appearance = ui.get_caption_appearance();
    vec![
        check("文字色", text, hex(appearance.text_color)),
        check("縁取りの色", outline, hex(appearance.outline_color)),
        check("縁取りの太さ", width, appearance.outline_width.to_string()),
    ]
}

pub(super) fn run(
    ui: &EditorWindow,
    editor: &Rc<RefCell<Editor>>,
    source: PathBuf,
    output: PathBuf,
) -> Result<()> {
    editor.borrow_mut().dialogs.supply([]);
    fs::create_dir_all(&output)?;
    let workspace = tempfile::tempdir()?;
    let destination = workspace.path().join("fixture.json");
    let mut specs = spec::load()?;
    let bdd_scenarios = specs.len();
    specs.extend(
        editor_controls::CASES
            .iter()
            .map(|&(id, name, steps)| spec::ScenarioSpec {
                evidence_id: id,
                name: name.to_owned(),
                stages: (0..steps).map(|_| Vec::new()).collect(),
            }),
    );
    let extra_output = output.join("editor");
    fs::create_dir_all(&extra_output)?;
    let mut report = Report {
        input_method: "window-events",
        scenarios: specs
            .iter()
            .map(|spec| Scenario {
                evidence_id: spec.evidence_id,
                name: spec.name.clone(),
                steps: Vec::new(),
            })
            .collect(),
    };
    let mut scenario = 0;
    let mut step = 0;
    let mut pending: Option<(Step, Instant)> = None;
    let error = Rc::new(RefCell::new(None));
    let deadline = Instant::now() + Duration::from_secs(85);
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
                {
                    let mut editor = editor.borrow_mut();
                    editor.poll_export(&ui);
                    editor.tick(&ui)?;
                    if let Err(error) = editor.poll_chat(&ui) {
                        ui.set_ai_status(format!("{error:#}").into());
                    }
                }
                if let Some((capture, ready_at)) = pending.as_ref() {
                    let failed = capture
                        .checks
                        .iter()
                        .any(|check| check.expected != check.actual);
                    let editor_pending = {
                        let editor = editor.borrow();
                        editor.last_frame.is_none() || editor.seek.is_pending()
                    };
                    let operation_pending = scenario >= bdd_scenarios
                        && editor_controls::awaiting(
                            &editor.borrow(),
                            scenario - bdd_scenarios,
                            step,
                        );
                    if Instant::now() < *ready_at
                        || (!failed
                            && (editor_pending || operation_pending)
                            && Instant::now() < *ready_at + Duration::from_secs(12))
                    {
                        return Ok(false);
                    }
                    let directory = if scenario < bdd_scenarios {
                        &output
                    } else {
                        &extra_output
                    };
                    capture_window(&ui, &directory.join(&capture.screenshot))?;
                    let mut capture = pending.take().unwrap().0;
                    if !failed && editor_pending {
                        capture.checks.push(check(
                            "新しいプレビューフレーム",
                            "準備完了",
                            "時間切れ",
                        ));
                    }
                    if scenario >= bdd_scenarios {
                        editor_controls::observe(
                            &ui,
                            &editor.borrow(),
                            &destination,
                            scenario - bdd_scenarios,
                            step,
                            &mut capture,
                        );
                    }
                    if scenario == 4 {
                        capture.checks.push(check(
                            "プレビューの字幕本文",
                            "実況字幕 1",
                            ui.get_preview_caption(),
                        ));
                    }
                    let failed = capture
                        .checks
                        .iter()
                        .any(|check| check.expected != check.actual);
                    report.scenarios[scenario].steps.push(capture);
                    step += 1;
                    if failed || step == specs[scenario].stages.len() {
                        ui.invoke_report_reset_panels();
                        let mut editor = editor.borrow_mut();
                        editor.dialogs.supply([]);
                        editor.leaving = None;
                        editor.report_ai = None;
                        scenario += 1;
                        step = 0;
                    }
                    if scenario == report.scenarios.len() {
                        let extra = Report {
                            input_method: report.input_method,
                            scenarios: report.scenarios.split_off(bdd_scenarios),
                        };
                        for (directory, contents) in [(&output, &report), (&extra_output, &extra)] {
                            let file = fs::File::create(directory.join("report.json"))?;
                            serde_json::to_writer_pretty(file, &contents)?;
                        }
                        ensure!(
                            report
                                .scenarios
                                .iter()
                                .chain(&extra.scenarios)
                                .flat_map(|scenario| &scenario.steps)
                                .flat_map(|step| &step.checks)
                                .all(|check| check.expected == check.actual),
                            "履歴の画面検査に失敗しました"
                        );
                        return Ok(true);
                    }
                    return Ok(false);
                }
                let operation = if scenario < bdd_scenarios {
                    perform(&ui, &editor, &source, &destination, scenario, step)
                } else {
                    editor_controls::execute(
                        &ui,
                        &editor,
                        &source,
                        &destination,
                        scenario - bdd_scenarios,
                        step,
                    )
                };
                let mut capture = operation.unwrap_or_else(|error| {
                    eprintln!(
                        "{} の操作 {}: {error:#}",
                        report.scenarios[scenario].name,
                        step + 1
                    );
                    Step {
                        action: "画面操作を実行する",
                        operations: &[],
                        rationale: "操作の実行中に検査が中断されました。",
                        checks: vec![check("画面操作", "成功", error.to_string())],
                        screenshot: format!("{}-{}.png", scenario + 1, step + 1),
                        spec: Vec::new(),
                    }
                });
                if scenario >= bdd_scenarios {
                    capture.screenshot =
                        format!("editor-{}-{}.png", scenario - bdd_scenarios + 1, step + 1);
                }
                capture.spec = std::mem::take(&mut specs[scenario].stages[step]);
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
