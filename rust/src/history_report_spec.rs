//! Bind authored history scenarios to their UI capture stages.

use anyhow::{Context, Result, ensure};
use gherkin::{Feature, GherkinEnv, StepType};
use serde::Serialize;

const FEATURE_PATH: &str = "docs/features/edit-history.feature";
const FEATURE: &str = include_str!("../../docs/features/edit-history.feature");
type Stage = &'static [(StepType, &'static str)];

// Order is shared with the capture driver. An empty stage records fixture setup
// without attributing a Gherkin step to it.
const CAPTURES: [(&str, &[Stage]); 2] = [
    (
        "ui-redo-after-edit",
        &[
            &[],
            &[(
                StepType::Given,
                "制作者が以前のクリップ変更を元に戻し、Redo ができる",
            )],
            &[
                (
                    StepType::When,
                    "制作者が代わりに字幕を修正し、その変更を適用する",
                ),
                (StepType::Then, "字幕の修正が現在の編集状態に反映される"),
                (StepType::Then, "通常の Redo は利用できなくなる"),
            ],
        ],
    ),
    (
        "ui-reopen-history",
        &[
            &[(
                StepType::Given,
                "制作者が字幕本文を「ボス戦」に編集し、続けてクリップの区間を変更した",
            )],
            &[(
                StepType::Given,
                "区間変更だけを元に戻してプロジェクトを保存した",
            )],
            &[
                (StepType::When, "制作者が保存したプロジェクトを開き直す"),
                (
                    StepType::Then,
                    "字幕は「ボス戦」で、クリップの区間は Undo 後の値である",
                ),
                (StepType::Then, "「元に戻す」と「やり直す」の両方を使える"),
            ],
            &[
                (StepType::When, "制作者が「やり直す」を押す"),
                (StepType::Then, "保存前に元に戻した区間変更が適用される"),
            ],
        ],
    ),
];

pub(super) struct ScenarioSpec {
    pub name: String,
    pub stages: Vec<Vec<SpecStep>>,
}

#[derive(Serialize)]
pub(super) struct SpecStep {
    keyword: String,
    text: String,
}

pub(super) fn load() -> Result<Vec<ScenarioSpec>> {
    parse(FEATURE)
}

fn parse(source: &str) -> Result<Vec<ScenarioSpec>> {
    let feature = Feature::parse(source, GherkinEnv::default()).context(FEATURE_PATH)?;
    ensure!(
        feature.background.is_none(),
        "{FEATURE_PATH}: the UI capture driver has no feature Background binding"
    );
    CAPTURES
        .iter()
        .map(|&(tag, stages)| bind_scenario(&feature, tag, stages))
        .collect()
}

fn bind_scenario(feature: &Feature, tag: &str, stages: &[Stage]) -> Result<ScenarioSpec> {
    let mut matches = feature
        .scenarios
        .iter()
        .filter(|scenario| scenario.tags.iter().any(|value| value == tag));
    let scenario = matches
        .next()
        .with_context(|| format!("{FEATURE_PATH}: missing UI capture scenario @{tag}"))?;
    ensure!(
        matches.next().is_none(),
        "{FEATURE_PATH}: duplicate UI capture scenario @{tag}"
    );
    ensure!(
        !scenario.tags.iter().any(|value| value == "planned") && scenario.examples.is_empty(),
        "{FEATURE_PATH}: @{tag} must be an executable scenario without Examples"
    );
    ensure!(
        scenario.steps.len() == stages.iter().map(|stage| stage.len()).sum::<usize>(),
        "{FEATURE_PATH}: step count changed for @{tag}; update its UI capture binding"
    );

    let mut captured_stages = Vec::new();
    let mut offset = 0;
    for stage in stages {
        let end = offset + stage.len();
        let mut captured_steps = Vec::new();
        for (&(kind, expected), step) in stage.iter().zip(&scenario.steps[offset..end]) {
            ensure!(
                step.ty == kind
                    && step.value == expected
                    && step.table.is_none()
                    && step.docstring.is_none(),
                "{FEATURE_PATH}:{}: @{tag} no longer matches its UI capture binding; update the captured action or check for {:?}",
                step.position.line,
                step.value
            );
            captured_steps.push(SpecStep {
                keyword: step.keyword.trim().to_owned(),
                text: step.value.clone(),
            });
        }
        captured_stages.push(captured_steps);
        offset = end;
    }
    Ok(ScenarioSpec {
        name: scenario.name.clone(),
        stages: captured_stages,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_and_authored_keywords_come_from_gherkin() -> Result<()> {
        let cases = parse(&FEATURE.replacen(
            "Undo の後の新しい編集で通常の Redo の候補を消す",
            "変更したシナリオ名",
            1,
        ))?;
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].name, "変更したシナリオ名");
        assert!(cases[0].stages[0].is_empty());
        let keywords: Vec<_> = cases[0].stages[2]
            .iter()
            .map(|step| step.keyword.as_str())
            .collect();
        assert_eq!(keywords, ["もし", "ならば", "かつ"]);
        assert_eq!(cases[0].stages[2][2].text, "通常の Redo は利用できなくなる");
        Ok(())
    }

    #[test]
    fn changed_expectation_cannot_reuse_unrelated_ui_capture() {
        let changed = FEATURE.replacen(
            "通常の Redo は利用できなくなる",
            "通常の Redo は利用できる",
            1,
        );
        let error = parse(&changed)
            .err()
            .expect("the binding rejects a changed result");
        assert!(
            error
                .to_string()
                .contains("no longer matches its UI capture binding")
        );
    }
}
