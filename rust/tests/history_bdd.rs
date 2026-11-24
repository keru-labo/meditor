//! Runs the edit-history Gherkin feature against the editor's history and project APIs.

use anyhow::{Context, Result, bail, ensure};
use gherkin::{Feature, GherkinEnv, Scenario, Step, StepType};
use meditor_playback::{
    history::{Edit, EditHistory, EditTarget},
    project::Project,
};
use regex::Regex;
use std::collections::BTreeMap;

#[path = "support/history.rs"]
mod fixture;

use fixture::{DURATION, TRIM, caption, project};

const FEATURE: &str = "docs/features/edit-history.feature";
const PLANNED: [(&str, usize); 5] = [
    ("保存したチャットと作業履歴から編集を再開する", 1),
    ("GUI とチャットで同じ作業履歴を元に戻し、やり直す", 2),
    (
        "後から行った独立した字幕修正を残してカット変更だけ差し戻す",
        1,
    ),
    ("差し戻しが後続の編集に影響する場合は確認を待つ", 2),
    ("Redo の候補から外れた変更を作業履歴から確認する", 1),
];
const REDO_SCENARIO: &str = "Undo の後の新しい編集で通常の Redo の候補を消す";
const REDO_ERROR: &str = "redo remains available after a new edit";

#[test]
fn history_bdd() -> Result<()> {
    run_feature(false)
}

#[test]
#[ignore = "run against the temporary copy with redo clearing removed"]
fn history_bdd_detects_redo_regression() -> Result<()> {
    run_feature(true)
}

fn run_feature(expect_regression: bool) -> Result<()> {
    let source = include_str!("../../docs/features/edit-history.feature");
    let feature = Feature::parse(source, GherkinEnv::default()).context(FEATURE)?;
    let placeholder = Regex::new(r"<[^\s<>]+>")?;
    let mut planned = BTreeMap::new();
    let mut executed = 0;
    let mut detected = 0;
    let mut failures = Vec::new();

    let scenarios = feature
        .scenarios
        .iter()
        .map(|scenario| (scenario, None))
        .chain(feature.rules.iter().flat_map(|rule| {
            rule.scenarios
                .iter()
                .map(move |scenario| (scenario, Some(rule)))
        }));
    for (scenario, rule) in scenarios {
        let mut tags = feature
            .tags
            .iter()
            .chain(&scenario.tags)
            .chain(rule.into_iter().flat_map(|rule| &rule.tags));
        let is_planned = tags.any(|tag| tag == "planned");
        let backgrounds = feature
            .background
            .iter()
            .chain(rule.and_then(|rule| rule.background.as_ref()));
        let steps: Vec<&Step> = backgrounds
            .flat_map(|background| &background.steps)
            .chain(&scenario.steps)
            .collect();

        for case in scenario_cases(scenario).with_context(|| Case::default().label(scenario))? {
            let label = case.label(scenario);
            if is_planned || case.planned {
                *planned.entry(scenario.name.as_str()).or_insert(0) += 1;
                println!("PLANNED {label}");
                continue;
            }

            let should_detect = expect_regression && scenario.name == REDO_SCENARIO;
            match (should_detect, run_case(&steps, &case, &placeholder)) {
                (true, Err(error)) if error.to_string() == REDO_ERROR => {
                    detected += 1;
                    println!("DETECTED {label}");
                }
                (true, Ok(())) => {
                    failures.push(format!("{label}: redo regression was not detected"))
                }
                (_, Err(error)) => failures.push(format!("{label}: {error:#}")),
                (false, Ok(())) => println!("PASS {label}"),
            }
            executed += 1;
        }
    }

    ensure!(failures.is_empty(), "{}", failures.join("\n"));
    ensure!(
        planned == BTreeMap::from(PLANNED),
        "unexpected @planned scenarios: {planned:?}"
    );
    ensure!(executed > 0, "no executable edit-history scenarios");
    ensure!(
        !expect_regression || detected == 1,
        "expected to detect the redo regression once, detected {detected} times"
    );
    Ok(())
}

#[derive(Default)]
struct Case<'a> {
    example: Option<(usize, usize)>,
    columns: &'a [String],
    values: &'a [String],
    planned: bool,
}

impl Case<'_> {
    fn label(&self, scenario: &Scenario) -> String {
        let mut label = format!("{FEATURE}:{}: {}", scenario.position.line, scenario.name);
        if let Some((line, row)) = self.example {
            label.push_str(&format!(" (Examples at line {line}, row {row})"));
        }
        label
    }
}

fn scenario_cases(scenario: &Scenario) -> Result<Vec<Case<'_>>> {
    if scenario.examples.is_empty() {
        return Ok(vec![Case::default()]);
    }

    let mut cases = Vec::new();
    for examples in &scenario.examples {
        let rows = &examples
            .table
            .as_ref()
            .context("Examples has no table")?
            .rows;
        let Some((columns, values)) = rows.split_first() else {
            bail!("Examples has no header");
        };
        ensure!(!values.is_empty(), "Examples has no rows");
        for (index, values) in values.iter().enumerate() {
            cases.push(Case {
                example: Some((examples.position.line, index + 1)),
                columns,
                values,
                planned: examples.tags.iter().any(|tag| tag == "planned"),
            });
        }
    }
    Ok(cases)
}

fn run_case(steps: &[&Step], case: &Case<'_>, placeholder: &Regex) -> Result<()> {
    ensure!(
        steps.iter().any(|step| step.ty == StepType::Then),
        "scenario has no expected outcome"
    );

    let directory = tempfile::tempdir()?;
    let path = directory.path().join("project.json");
    let mut project = project()?;
    let mut history = EditHistory::default();

    for step in steps {
        ensure!(
            step.table.is_none() && step.docstring.is_none(),
            "step arguments have no runner support at line {}",
            step.position.line
        );
        let text = placeholder.replace_all(&step.value, |captures: &regex::Captures<'_>| {
            let parameter = &captures[0];
            case.columns
                .iter()
                .zip(case.values)
                .find(|(column, _)| column.as_str() == &parameter[1..parameter.len() - 1])
                .map_or_else(|| parameter.to_owned(), |(_, value)| value.clone())
        });
        ensure!(
            !placeholder.is_match(&text),
            "unresolved Examples placeholder: {text}"
        );
        match text.as_ref() {
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
                ensure!(!history.can_redo(), REDO_ERROR);
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
            _ => bail!(
                "undefined Gherkin step at line {}: {text}",
                step.position.line
            ),
        }
    }

    Ok(())
}
