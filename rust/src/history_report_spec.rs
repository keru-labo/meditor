//! Bind authored history scenarios to their UI capture stages.

use anyhow::{Context, Result, ensure};
use gherkin::{Feature, GherkinEnv, StepType};
use serde::Serialize;

const FEATURE_PATH: &str = "docs/features/edit-history.feature";
const FEATURE: &str = include_str!("../../docs/features/edit-history.feature");
const CLIP_PATH: &str = "docs/features/ai-clip-editing.feature";
const SUBTITLE_PATH: &str = "docs/features/subtitle-editing.feature";
type Stage = &'static [(StepType, &'static str)];
type Binding = (&'static str, &'static [Stage]);

// Order is shared with the capture driver. An empty stage records fixture setup
// without attributing a Gherkin step to it.
const CAPTURES: [Binding; 2] = [
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

const CLIP_BACKGROUND: (StepType, &str) = (
    StepType::Given,
    "90 秒の実況動画と対応するゲーム音・話者音声がプロジェクトから参照できる",
);
const CLIP_CAPTURES: [Binding; 2] = [
    (
        "ui-clip-trim-start",
        &[
            &[
                CLIP_BACKGROUND,
                (
                    StepType::Given,
                    "AI の編集結果として素材時刻 30 秒から 40 秒までのクリップがタイムライン時刻 0 秒にある",
                ),
                (
                    StepType::Given,
                    "ゲーム音のオフセットは 0 秒で、話者音声のオフセットはプラス 2 秒である",
                ),
            ],
            &[
                (
                    StepType::When,
                    "制作者が GUI でクリップの開始を素材時刻 32 秒に調整する",
                ),
                (
                    StepType::Then,
                    "クリップはタイムライン時刻 0 秒から 8 秒までになる",
                ),
                (
                    StepType::Then,
                    "タイムライン時刻 1 秒では動画とゲーム音の素材時刻 33 秒および話者音声の素材時刻 31 秒が同期する",
                ),
            ],
            &[
                (StepType::When, "制作者がプロジェクトを保存して開き直す"),
                (
                    StepType::Then,
                    "素材時刻 32 秒から 40 秒までのクリップがタイムライン時刻 0 秒に復元される",
                ),
            ],
        ],
    ),
    (
        "ui-clip-trim-end",
        &[
            &[
                CLIP_BACKGROUND,
                (
                    StepType::Given,
                    "AI の編集結果として素材時刻 30 秒から 40 秒までのクリップがタイムライン時刻 0 秒にある",
                ),
            ],
            &[
                (
                    StepType::When,
                    "制作者が GUI でクリップの終了を素材時刻 38 秒に調整する",
                ),
                (
                    StepType::Then,
                    "素材時刻 30 秒から 38 秒までのクリップがタイムライン時刻 0 秒から 8 秒までを占める",
                ),
            ],
        ],
    ),
];

const SUBTITLE_CAPTURES: [Binding; 1] = [(
    "ui-subtitle-style",
    &[
        &[(
            StepType::Given,
            "プレビューに字幕が表示されているプロジェクトがある",
        )],
        &[
            (
                StepType::When,
                "制作者が字幕スタイルで文字色を黄色、縁取りを青く太くして適用する",
            ),
            (
                StepType::Then,
                "現在のプレビューと、それ以降に書き出す MP4 の字幕に指定した色と縁取りが反映される",
            ),
            (StepType::Then, "字幕の周囲に矩形の背景は描かれない"),
        ],
        &[
            (StepType::When, "制作者がその変更を元に戻す"),
            (StepType::Then, "文字色と縁取りが一度に適用前の設定へ戻る"),
        ],
    ],
)];

pub(super) struct ScenarioSpec {
    pub evidence_id: &'static str,
    pub name: String,
    pub stages: Vec<Vec<SpecStep>>,
}

pub(super) const INVALID_CLIP_INPUTS: [(&str, &str, &str); 4] = [
    ("ui-clip-invalid-start-negative", "開始", "-1"),
    ("ui-clip-invalid-end-outside", "終了", "91"),
    ("ui-clip-invalid-start-equal", "開始", "40"),
    ("ui-clip-invalid-end-before", "終了", "31"),
];
const INVALID_CLIP_STAGES: &[Stage] = &[
    &[
        CLIP_BACKGROUND,
        (
            StepType::Given,
            "素材時刻 32 秒から 40 秒までのクリップがタイムライン時刻 0 秒にある",
        ),
    ],
    &[
        (
            StepType::When,
            "制作者が GUI でクリップの<境界>を素材時刻 <秒> 秒に調整しようとする",
        ),
        (
            StepType::Then,
            "素材時刻 32 秒から 40 秒までのクリップがタイムライン時刻 0 秒に保持される",
        ),
    ],
];

#[derive(Serialize)]
pub(super) struct SpecStep {
    keyword: String,
    text: String,
}

pub(super) fn load() -> Result<Vec<ScenarioSpec>> {
    let mut scenarios = parse(FEATURE, FEATURE_PATH, &CAPTURES)?;
    let clip_source = include_str!("../../docs/features/ai-clip-editing.feature");
    scenarios.extend(parse(clip_source, CLIP_PATH, &CLIP_CAPTURES)?);
    scenarios.extend(parse(
        include_str!("../../docs/features/subtitle-editing.feature"),
        SUBTITLE_PATH,
        &SUBTITLE_CAPTURES,
    )?);
    let clip = Feature::parse(clip_source, GherkinEnv::default()).context(CLIP_PATH)?;
    let outline = scenario_with_tag(&clip, CLIP_PATH, "ui-clip-invalid")?;
    let rows = outline.examples.as_slice();
    let [examples] = rows else {
        anyhow::bail!("{CLIP_PATH}: expected one Examples table for @ui-clip-invalid");
    };
    let table = examples
        .table
        .as_ref()
        .context("Missing invalid clip Examples table")?;
    let mut rows = table
        .rows
        .iter()
        .map(|row| row.iter().map(String::as_str).collect::<Vec<_>>());
    ensure!(
        rows.next() == Some(vec!["境界", "秒"]),
        "{CLIP_PATH}: unexpected invalid clip Examples columns"
    );
    let actual: Vec<_> = rows.collect();
    let expected: Vec<_> = INVALID_CLIP_INPUTS
        .iter()
        .map(|&(_, control, value)| vec![control, value])
        .collect();
    ensure!(
        actual == expected,
        "{CLIP_PATH}: invalid clip Examples changed; update the screen capture inputs"
    );
    for (tag, control, value) in INVALID_CLIP_INPUTS {
        let mut scenario = bind_steps(&clip, outline, CLIP_PATH, tag, INVALID_CLIP_STAGES)?;
        for step in scenario.stages.iter_mut().flatten() {
            step.text = step.text.replace("<境界>", control).replace("<秒>", value);
        }
        scenarios.push(scenario);
    }
    Ok(scenarios)
}

fn parse(source: &str, path: &str, captures: &[Binding]) -> Result<Vec<ScenarioSpec>> {
    let feature = Feature::parse(source, GherkinEnv::default()).with_context(|| path.to_owned())?;
    captures
        .iter()
        .map(|&(tag, stages)| bind_scenario(&feature, path, tag, stages))
        .collect()
}

fn bind_scenario(
    feature: &Feature,
    path: &str,
    tag: &'static str,
    stages: &[Stage],
) -> Result<ScenarioSpec> {
    let scenario = scenario_with_tag(feature, path, tag)?;
    ensure!(
        scenario.examples.is_empty(),
        "{path}: @{tag} must be an executable scenario without Examples"
    );
    bind_steps(feature, scenario, path, tag, stages)
}

fn scenario_with_tag<'a>(
    feature: &'a Feature,
    path: &str,
    tag: &str,
) -> Result<&'a gherkin::Scenario> {
    let mut matches = feature
        .scenarios
        .iter()
        .filter(|scenario| scenario.tags.iter().any(|value| value == tag));
    let scenario = matches
        .next()
        .with_context(|| format!("{path}: missing UI capture scenario @{tag}"))?;
    ensure!(
        matches.next().is_none(),
        "{path}: duplicate UI capture scenario @{tag}"
    );
    ensure!(
        !scenario.tags.iter().any(|value| value == "planned"),
        "{path}: @{tag} must be an executable scenario"
    );
    Ok(scenario)
}

fn bind_steps(
    feature: &Feature,
    scenario: &gherkin::Scenario,
    path: &str,
    tag: &'static str,
    stages: &[Stage],
) -> Result<ScenarioSpec> {
    let steps: Vec<_> = feature
        .background
        .iter()
        .flat_map(|background| &background.steps)
        .chain(&scenario.steps)
        .collect();
    ensure!(
        steps.len() == stages.iter().map(|stage| stage.len()).sum::<usize>(),
        "{path}: step count changed for @{tag}; update its UI capture binding"
    );

    let mut captured_stages = Vec::new();
    let mut offset = 0;
    for stage in stages {
        let end = offset + stage.len();
        let mut captured_steps = Vec::new();
        for (&(kind, expected), step) in stage.iter().zip(&steps[offset..end]) {
            ensure!(
                step.ty == kind
                    && step.value == expected
                    && step.table.is_none()
                    && step.docstring.is_none(),
                "{path}:{}: @{tag} no longer matches its UI capture binding; update the captured action or check for {:?}",
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
        evidence_id: tag,
        name: scenario.name.clone(),
        stages: captured_stages,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_and_authored_keywords_come_from_gherkin() -> Result<()> {
        let cases = parse(
            &FEATURE.replacen(
                "Undo の後の新しい編集で通常の Redo の候補を消す",
                "変更したシナリオ名",
                1,
            ),
            FEATURE_PATH,
            &CAPTURES,
        )?;
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].evidence_id, "ui-redo-after-edit");
        assert_eq!(cases[1].evidence_id, "ui-reopen-history");
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
        let error = parse(&changed, FEATURE_PATH, &CAPTURES)
            .err()
            .expect("the binding rejects a changed result");
        assert!(
            error
                .to_string()
                .contains("no longer matches its UI capture binding")
        );
    }
}
