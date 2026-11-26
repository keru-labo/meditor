#!/usr/bin/env python3
"""Build the public history report from a CI ZIP containing data and screenshots."""

import argparse
import html
import json
import re
import struct
import zipfile
from dataclasses import dataclass
from pathlib import Path

from gherkin_report_catalog import INVALID_CLIP_EXAMPLES, read_catalog

PNG_HEADER = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR"
CAPTURE_STAGE_COUNTS = {
    "ui-redo-after-edit": 3,
    "ui-reopen-history": 4,
    "ui-clip-trim-start": 3,
    "ui-clip-trim-end": 2,
    "ui-subtitle-style": 3,
    **dict.fromkeys(INVALID_CLIP_EXAMPLES.values(), 2),
}
CAPTURE_SCOPE_NOTES = {
    "ui-clip-trim-start": "画面検査の対象は素材区間、タイムラインの長さ、保存後の復元です。音声同期は別途確認が必要です。",
    "ui-clip-trim-end": "素材区間をあらかじめ設定した合成プロジェクトで、終了位置とタイムラインの長さを確認します。",
    "ui-subtitle-style": "画面検査の対象はプレビューの字幕と設定値、Undo での復元です。MP4の出力と背景の画素は別途確認が必要です。",
    **dict.fromkeys(INVALID_CLIP_EXAMPLES.values(), "合成プロジェクトで入力時のエラー表示と、適用済みの区間が保持されることを確認します。"),
}
IMAGE_LIMIT = 2 * 1024 * 1024
EDITOR_CASES = {
    "editor-project": ("プロジェクト作成と未保存時の確認", 7),
    "editor-playback": ("再生・一時停止・シーク", 6),
    "editor-captions": ("SRTの取り込みと字幕一覧の操作", 6),
    "editor-panels": ("アプリ情報・AI・書き出し設定の開閉", 6),
    "editor-proxy": ("プロキシの選択と切り替え", 3),
    "editor-metrics": ("計測の開始・停止・保存", 3),
    "editor-export": ("書き出し設定の選択とMP4保存", 8),
    "editor-transcription": ("ローカル文字起こしの開始・中止・保存", 4),
    "editor-ai": ("合成AI応答の適用・設定確認・中断", 9),
    "editor-ai-errors": ("AIの該当なし・無効応答・失敗の表示", 8),
    "editor-subtitle-pixels": ("字幕と縁取りをプレビューとMP4の画素で確認する", 4),
}
EDITOR_SCOPES = {
    "editor-ai-errors": "エラーを返す合成応答で、アプリによる表示と編集内容の保持を確認します。",
    "editor-ai": "アプリ内のAI結果は合成応答で再現しています。実アカウントへの接続とモデルの回答品質は検査範囲に含みません。",
    "editor-transcription": "文字起こしエンジンはSRTを返すテスト用CLIです。実モデルの認識精度は検査範囲に含みません。",
}


@dataclass
class Evidence:
    failed: bool
    stages: list[str]
    window_events: bool = False


def require(condition, message):
    if not condition:
        raise ValueError(message)


def text(value):
    require(isinstance(value, str) and len(value) <= 512, "Invalid report text")
    return html.escape(value)


def read_report(archive, destination, catalogue, stage_counts=CAPTURE_STAGE_COUNTS, png_prefix=""):
    files = {file.filename: file for file in archive.infolist()}
    require(
        len(files) == len(archive.infolist())
        and all(re.fullmatch(rf"report\.json|{re.escape(png_prefix)}[1-9][0-9]?-[1-9][0-9]?\.png", name) for name in files),
        "Unexpected report files",
    )
    require("report.json" in files and files["report.json"].file_size <= 64 * 1024, "Invalid report JSON")
    report = json.loads(archive.read("report.json"))
    results = report["scenarios"]
    require(isinstance(results, list) and len(results) == len(stage_counts), "Invalid scenarios")
    cases_by_tag = {
        case["evidence_id"]: case
        for feature in catalogue
        for case in feature["scenarios"]
        if case["evidence_id"] is not None
    }
    evidence = {}
    used = {"report.json"}
    for index, scenario in enumerate(results, 1):
        tag = scenario["evidence_id"]
        require(tag in stage_counts and tag in cases_by_tag, "Unknown UI capture tag")
        case = cases_by_tag[tag]
        require(case["id"] not in evidence, f"Duplicate UI capture tag: {tag}")
        require(scenario["name"] == case["name"], f"UI capture title differs from Gherkin: {tag}")
        maximum = stage_counts[tag]
        steps = scenario["steps"]
        require(isinstance(steps, list) and 0 < len(steps) <= maximum, "Invalid steps")
        rendered = []
        captured_spec = []
        failed = False
        for position, step in enumerate(steps, 1):
            require(not failed, "Unexpected UI stage after a failure")
            checks = step["checks"]
            require(isinstance(checks, list) and 0 < len(checks) <= 6, "Invalid checks")
            rows = []
            for check in checks:
                passed = check["expected"] == check["actual"]
                failed |= not passed
                rows.append(
                    f"<tr><th scope='row'>{text(check['name'])}</th>"
                    f"<td>{text(check['expected'])}</td><td>{text(check['actual'])}</td>"
                    f"<td class='{'pass' if passed else 'fail'}'>{'成功' if passed else '失敗'}</td></tr>"
                )
            name = f"{png_prefix}{index}-{position}.png"
            require(step["screenshot"] == name and files[name].file_size <= IMAGE_LIMIT, "Invalid image")
            png = archive.read(name)
            require(png.startswith(PNG_HEADER) and len(png) >= 24, "Expected a screenshot PNG")
            width, height = struct.unpack(">II", png[16:24])
            require(0 < width <= 4096 and 0 < height <= 2160, "Invalid screenshot dimensions")
            (destination / name).write_bytes(png)
            used.add(name)
            action = text(step["action"])
            operations = step.get("operations", [])
            require(isinstance(operations, list) and len(operations) <= 6, "Invalid operations")
            instructions = (
                "<ol class='instructions'>" + "".join(f"<li>{text(item)}</li>" for item in operations) + "</ol>"
                if operations else ""
            )
            specification = step.get("spec", [])
            require(isinstance(specification, list) and len(specification) <= 12, "Invalid specification")
            captured_spec.extend((item["keyword"], item["text"]) for item in specification)
            spec = (
                "<dl class='specification'>" + "".join(
                    f"<div><dt>{text(item['keyword'])}</dt><dd>{text(item['text'])}</dd></div>"
                    for item in specification
                ) + "</dl>" if specification and tag not in CAPTURE_SCOPE_NOTES else ""
            )
            step_passed = all(check["expected"] == check["actual"] for check in checks)
            rationale = text(step.get("rationale", "画面から取得した値を、操作後に期待する値と照合しています。"))
            verdict = "なぜ成功か" if step_passed else "何が期待と違うか"
            result = "すべての確認点が期待値と一致しました。" if step_passed else "期待値と一致しなかった確認点があります。"
            rendered.append(
                f"<div class='action'><h3>{action}</h3>{spec}{instructions}</div>"
                "<figure><figcaption><span>操作直後の画面</span>"
                f"<a href='{name}' target='_blank' rel='noopener'>原寸で開く ↗</a></figcaption>"
                f"<img src='{name}' width='{width}' height='{height}' loading='lazy' alt='{action}直後のエディター画面'></figure>"
                f"<div class='checks'><h4 class='{'pass' if step_passed else 'fail'}'>{verdict}</h4>"
                f"<p>{result} {rationale}</p><table><thead><tr>"
                "<th scope='col'>確認点</th><th scope='col'>期待値</th><th scope='col'>実際の値</th>"
                f"<th scope='col'>結果</th></tr></thead><tbody>{''.join(rows)}</tbody></table></div>",
            )
        require(failed or len(steps) == maximum, "Incomplete passing scenario")
        require(all(not step["argument"] for step in case["steps"]), f"UI capture has unbound step arguments: {tag}")
        authored_spec = [(step["keyword"], step["text"]) for step in case["steps"]]
        require(
            authored_spec[:len(captured_spec)] == captured_spec
            and (failed or authored_spec == captured_spec),
            f"UI capture steps differ from Gherkin: {tag}",
        )
        evidence[case["id"]] = Evidence(failed, rendered, report.get("input_method") == "window-events")
    require(used == files.keys(), "Unexpected screenshot")
    return evidence


def editor_catalogue():
    return {
        "name": "エディターのUI操作",
        "uri": "",
        "scenarios": [
            dict(id=case_id, name=name, line=0, rule=None, description="", planned=False,
                 evidence_id=case_id, steps=[], example=None)
            for case_id, (name, _) in EDITOR_CASES.items()
        ],
    }


def timeline(items, label, prefix):
    cards = [
        f"<li class='step' id='{prefix}-{position}'>"
        f"<span class='step-number' aria-hidden='true'>{position}</span>{content}</li>"
        for position, content in enumerate(items, 1)
    ]
    return f"<ol class='steps' aria-label='{label}'>{''.join(cards)}</ol>"


def render_spec(step):
    background = "<span class='background'>背景</span>" if step["background"] else ""
    keyword = html.escape(step["keyword"])
    content = html.escape(step["text"])
    markup = f"<h4 class='gherkin-step'>{background}<span>{keyword}</span> {content}</h4>"
    argument = step["argument"]
    if not argument:
        return markup
    if argument["kind"] == "doc_string":
        media_type = argument["media_type"]
        caption = f"<p class='argument-type'>{html.escape(media_type)}</p>" if media_type else ""
        return markup + f"{caption}<pre>{html.escape(argument['content'])}</pre>"
    rows = [
        "<tr>" + "".join(f"<td>{html.escape(cell)}</td>" for cell in row) + "</tr>"
        for row in argument["rows"]
    ]
    return markup + f"<div class='checks'><table aria-label='ステップのデータ'><tbody>{''.join(rows)}</tbody></table></div>"


def render(catalogue, evidence, pr, sha, run):
    groups = []
    panels = []
    failures = []
    verified = []
    total = 0
    for feature in catalogue:
        feature_name = html.escape(feature["name"])
        links = []
        for case in feature["scenarios"]:
            total += 1
            scenario_id = f"scenario-{case['id']}"
            name = html.escape(case["name"])
            source = f"{html.escape(feature['uri'])}:{case['line']}"
            rule = case["rule"]
            feature_context = f"{feature_name} / {html.escape(rule)}" if rule else feature_name
            description = case["description"]
            description_note = f"<p class='description'>{html.escape(description)}</p>" if description else ""
            related = [
                tag.removeprefix("@report-") for tag in case.get("tags", [])
                if tag.startswith("@report-")
            ]
            related_links = "".join(
                f"<p class='source'><a class='related-case' href='#scenario-{case_id}'>関連するUI検査 · "
                f"{html.escape(EDITOR_CASES[case_id][0])}</a></p>"
                for case_id in related if case_id in EDITOR_CASES and case_id in evidence
            )
            captures = evidence.get(case["id"])
            authored = bool(case["steps"])
            image_count = len(captures.stages) if captures else 0
            spec_id = f"{scenario_id}-spec"
            status, state = ("計画中", "neutral") if case["planned"] else ("画面の実行結果なし", "neutral")
            results = ""
            spec_link = ""
            screenshot_note = " · スクリーンショットなし"
            if captures:
                status, state = ("失敗", "fail") if captures.failed else ("成功", "pass")
                (failures if captures.failed else verified).append(scenario_id)
                screenshot_note = f" · 画像 {image_count} 枚"
                if authored:
                    spec_link = f" <a href='#{spec_id}'>Gherkinの手順へ</a>"
                input_note = (
                    "<p class='coverage-note'>エディターにクリックとキー入力を送信して検査しています。"
                    "OSのファイル選択には合成素材のパスを渡しています。</p>"
                    if captures.window_events else ""
                )
                results = (
                    "<h3 class='flow-title'>画面検査の操作と結果</h3>"
                    + input_note
                    + (f"<p class='coverage-note'>{scope_note}</p>" if (scope_note :=
                        (CAPTURE_SCOPE_NOTES.get(case["evidence_id"]) or EDITOR_SCOPES.get(case["evidence_id"]))) else "")
                    + timeline(captures.stages, "画面検査の操作と結果", f"{scenario_id}-stage")
                )
                if not captures.failed and case["evidence_id"] in CAPTURE_SCOPE_NOTES:
                    status = "画面で確認"
            example = case["example"]
            example_note = ""
            if example:
                values = "、".join(f"{key} = {value}" for key, value in example["values"])
                example_note = f"<p class='source'>例 {example['line']}行目 · {html.escape(values)}</p>"
            links.append(
                f"<a href='#{scenario_id}' data-images='{image_count}'><span>{name}</span>"
                f"<small class='{state}'>{status}"
                + (f" · {len(case['steps'])} ステップ" if authored else "")
                + screenshot_note + (f" · 例 {example['line']}行目" if example else "") + "</small></a>"
            )
            specification = (
                f"<h3 class='flow-title' id='{spec_id}'>Gherkinの手順</h3>"
                f"<p class='source'><code>{source}</code></p>{description_note}{example_note}"
                + timeline((render_spec(step) for step in case["steps"]), "Gherkinの手順", f"{scenario_id}-step")
                if authored else ""
            )
            panels.append(
                f"<section id='{scenario_id}' aria-labelledby='{scenario_id}-title'>"
                f"<p class='feature-path'>{feature_context}</p>"
                f"<h2 id='{scenario_id}-title' tabindex='-1'>{name}</h2>"
                f"<p class='scenario-meta'><strong class='{state}'>{status}</strong>{screenshot_note}{spec_link}</p>"
                f"{related_links}{results}{specification}</section>"
            )
        groups.append(
            f"<div class='feature-group'><h3>{feature_name}</h3>{''.join(links)}</div>"
        )
    preferred = (failures + verified + [""])[0]
    editor_total = sum(len(feature["scenarios"]) for feature in catalogue if not feature["uri"])
    extras = f" · UI操作 {editor_total} 件" if editor_total else ""
    summary = f"{total - editor_total} シナリオ{extras} · 画面検査の結果 {len(evidence)} 件"
    return f"""<!doctype html>
<html lang="ja">
<head>
<meta charset="utf-8">
<meta name="robots" content="noindex, nofollow, noarchive">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>BDDレポート · PR #{pr}</title>
<style>
* {{ box-sizing: border-box }}
:root {{
  color: #172540; background: #f4f6fa; font: 16px/1.65 system-ui, sans-serif;
  --line: #dbe3ee; --muted: #455572; --accent: #174ea5;
}}
[hidden] {{ display: none !important }}
html {{ scrollbar-gutter: stable }}
body {{ margin: 0 }}
a {{ color: var(--accent) }}
a:focus-visible {{ outline: 3px solid var(--accent); outline-offset: 3px }}
h1, h2, h3, h4, p {{ margin: 0 }}
h1, h2, h3 {{ line-height: 1.4; text-wrap: pretty }}
.topbar {{ background: #172540; color: #fff }}
.topbar-inner {{
  margin: auto; max-width: 94rem; padding: 1.25rem clamp(1rem, 4vw, 3rem);
  display: flex; justify-content: space-between; gap: .5rem 1.5rem; flex-wrap: wrap; align-items: baseline;
}}
.topbar h1 {{ font-size: clamp(1.2rem, 3vw, 1.65rem) }}
.topbar p {{ color: #c5d4ee; font-size: .87rem }}
.topbar a {{ color: #d8e9ff }}
.topbar .report-summary {{ margin-top: .2rem }}
.layout {{
  margin: auto; max-width: 76rem;
  padding: clamp(1rem, 4vw, 3rem) clamp(1rem, 4vw, 3rem) 3rem max(3.15rem, 4vw);
}}
.menu-trigger, .drawer-close {{
  font: inherit; cursor: pointer; border: 1px solid var(--line); font-weight: 650;
}}
.menu-trigger {{
  position: fixed; z-index: 2; top: 42%; left: 0;
  padding: .8rem .36rem; border-left: 0; border-radius: 0 .65rem .65rem 0;
  writing-mode: vertical-rl; background: #172540; color: #fff;
  box-shadow: 0 4px 16px #17254038;
}}
.menu-trigger:hover {{ background: #254673 }}
.menu-trigger:focus-visible, .drawer-close:focus-visible {{
  outline: 3px solid var(--accent); outline-offset: 3px;
}}
.scenario-drawer {{
  position: fixed; inset: 0 auto 0 0; margin: 0; padding: 0;
  width: min(22rem, calc(100vw - 2rem)); max-width: none; max-height: none;
  height: 100dvh; border: 0; border-right: 1px solid var(--line);
  background: #fff; color: inherit; box-shadow: 12px 0 36px #17254040;
  overflow: hidden;
  transform: translateX(-100%);
  transition: transform .24s ease-out, display .24s allow-discrete, overlay .24s allow-discrete;
}}
.scenario-drawer[open] {{ transform: translateX(0) }}
.drawer-content {{ height: 100%; display: flex; flex-direction: column }}
.scenario-drawer::backdrop {{ background: #17254055 }}
body:has(.scenario-drawer[open]) {{ overflow: hidden }}
@starting-style {{
  .scenario-drawer[open] {{ transform: translateX(-100%) }}
}}
@media (prefers-reduced-motion: reduce) {{
  .scenario-drawer {{ transition: none }}
}}
.drawer-toolbar {{ padding: 1.1rem; border-bottom: 1px solid var(--line) }}
.drawer-header {{
  display: flex; align-items: center; justify-content: space-between; gap: .75rem;
}}
.drawer-header h2 {{ font-size: 1.2rem }}
.drawer-close {{
  flex: none; padding: .32rem .65rem; border-radius: .4rem; background: #f4f6fa; color: inherit;
}}
.drawer-search {{
  width: 100%; margin-top: .65rem; padding: .5rem .65rem; font: inherit;
  border: 1px solid var(--line); border-radius: .4rem; color: inherit; background: #fff;
}}
.drawer-search:focus-visible {{ outline: 2px solid var(--accent); outline-offset: 1px }}
.drawer-filter {{ display: flex; align-items: center; gap: .4rem; margin-top: .65rem; font-size: .85rem; cursor: pointer }}
.scenarios {{ flex: 1; min-height: 0; overflow-y: auto; overscroll-behavior: contain; padding: .8rem 1.1rem 1.1rem }}
.feature-group + .feature-group {{ margin-top: 1.25rem }}
.feature-group h3 {{ font-size: .85rem; color: var(--muted); margin: .15rem 0 .5rem }}
.scenarios a {{
  display: block; padding: .85rem .95rem; margin-bottom: .5rem; border: 1px solid var(--line);
  border-radius: .6rem; background: #fff; color: inherit; text-decoration: none;
}}
.scenarios a:hover {{ background: #f1f6ff }}
.scenarios a[aria-current] {{ background: #edf4ff; border-color: #4780d1; box-shadow: inset 3px 0 #2563bd }}
.scenarios a span {{ display: block; font-weight: 650 }}
.scenarios small {{ font-size: .81rem }}
.pass {{ color: #167246 }} .fail {{ color: #a52830; font-weight: 650 }}
.neutral, .source, .feature-path, .scenario-meta code {{ color: var(--muted) }}
main, main section {{ min-width: 0 }}
main section {{ scroll-margin-top: 1rem }}
main h2 {{ font-size: clamp(1.35rem, 3vw, 2rem) }}
.feature-path {{ margin-bottom: .25rem; font-size: .9rem }}
.scenario-meta, .source {{ margin-top: .4rem; font-size: .86rem; overflow-wrap: anywhere }}
.scenario-meta a {{ margin-inline-start: .65rem }}
.source + .steps, .description + .steps {{ margin-top: .8rem }}
.description {{ margin-top: .75rem; white-space: pre-wrap; overflow-wrap: anywhere }}
.flow-title {{ margin: 1.65rem 0 .8rem; font-size: 1.13rem }}
.coverage-note {{ margin: 0 0 .8rem; color: var(--muted) }}
.steps {{
  position: relative; padding: 0 0 0 3rem; list-style: none;
  display: grid; gap: 1rem; margin: 0;
}}
.steps::before {{
  content: ""; position: absolute; inset: 0 auto 0 .9rem; width: 2px; background: var(--line);
}}
.step {{
  position: relative; padding: clamp(.85rem, 2vw, 1.5rem); scroll-margin-top: 1rem;
  border: 1px solid var(--line); border-radius: .85rem; background: #fff; min-width: 0;
}}
.step-number {{
  position: absolute; left: -3.08rem; top: 1.1rem; display: grid; place-items: center;
  border-radius: 50%; width: 1.95rem; height: 1.95rem;
  background: #e7f0ff; color: var(--accent); font-weight: 750;
}}
.action h3 {{ font-size: clamp(1.08rem, 2.2vw, 1.4rem) }}
.gherkin-step {{ font-weight: 550; overflow-wrap: anywhere }}
.gherkin-step span {{ color: var(--accent); margin-inline-end: .3em; font-weight: 700 }}
.gherkin-step .background {{ color: var(--muted); font-size: .78rem; margin-inline-end: .7em }}
.argument-type {{ font-size: .82rem; color: var(--muted); margin-top: .5rem }}
.step pre {{ margin-bottom: 0; padding: .75rem; border-radius: .35rem; background: #f4f6fa; overflow-x: auto }}
.step .gherkin-step + .checks {{ margin-top: .5rem }}
.specification {{ margin: .55rem 0 0; padding: .25rem .65rem; border-left: 2px solid var(--accent) }}
.specification div {{ display: flex; flex-wrap: wrap; gap: 0 .5rem }}
.specification dt {{ color: var(--accent); font-weight: 650 }}
.specification dd {{ margin: 0; overflow-wrap: anywhere }}
.instructions {{ color: var(--muted); padding-left: 1.15rem; margin: .45rem 0 0 }}
figure {{ margin: 1rem 0; background: #edf1f7; border: 1px solid var(--line); border-radius: .6rem; overflow: hidden }}
figcaption {{
  display: flex; justify-content: space-between; align-items: baseline; flex-wrap: wrap; gap: .2rem 1rem;
  padding: .55rem .85rem; color: var(--muted); font-size: .87rem; font-weight: 650;
}}
figcaption a {{ font-weight: 500 }}
img {{ display: block; width: 100%; height: auto; background: #172540 }}
.checks {{ overflow-x: auto }}
.checks h4 {{ font-size: 1rem; margin: .2rem 0 .35rem }}
.checks p {{ margin-bottom: .7rem }}
table {{ width: 100%; border-collapse: collapse; text-align: left; font-size: .9rem }}
th, td {{ padding: .4rem .6rem; border-bottom: 1px solid var(--line); overflow-wrap: anywhere }}
thead {{ background: #f3f6fb }}
tbody tr:last-child th, tbody tr:last-child td {{ border-bottom: 0 }}
@media (max-width: 720px) {{
  th, td {{ padding: .35rem }}
}}
</style>
</head>
<body>
<header class="topbar"><div class="topbar-inner"><div><h1>BDDレポート</h1><p class="report-summary">{summary}</p></div><p>
<a href="https://github.com/keru-labo/meditor/pull/{pr}">PR #{pr}</a> · <code>{sha[:12]}</code> ·
<a href="https://github.com/keru-labo/meditor/actions/runs/{run}">CIの実行結果</a></p></div></header>
<button class="menu-trigger" type="button" aria-controls="scenario-drawer" aria-expanded="false" aria-haspopup="dialog" hidden>シナリオ</button>
<dialog class="scenario-drawer" id="scenario-drawer" aria-labelledby="scenarios-title">
  <div class="drawer-content">
    <div class="drawer-toolbar">
      <div class="drawer-header"><h2 id="scenarios-title">シナリオ</h2><button class="drawer-close" type="button" autofocus>閉じる</button></div>
      <input class="drawer-search" type="search" aria-label="シナリオを検索" placeholder="機能名・シナリオ名で検索">
      <label class="drawer-filter"><input class="filter-images" type="checkbox">スクリーンショットあり</label>
    </div>
    <nav class="scenarios" aria-labelledby="scenarios-title">
      {"".join(groups)}
      <p class="no-matches" role="status" hidden>該当するシナリオがありません。</p>
    </nav>
  </div>
</dialog>
<main class="layout" data-preferred="{preferred}">{"".join(panels)}</main>
<script>
const trigger = document.querySelector('.menu-trigger');
const drawer = document.getElementById('scenario-drawer');
const links = [...drawer.querySelectorAll('.scenarios a')];
trigger.hidden = false;
trigger.addEventListener('click', () => {{
  drawer.showModal();
  trigger.setAttribute('aria-expanded', 'true');
  drawer.querySelector('[aria-current]:not([hidden])')?.scrollIntoView({{block: 'nearest'}});
}});
drawer.querySelector('.drawer-close').addEventListener('click', () => drawer.close());
drawer.addEventListener('keydown', event => {{
  if (event.key === 'Escape') {{
    event.preventDefault();
    drawer.close();
  }}
}});
drawer.addEventListener('click', event => {{
  if (event.target !== drawer) return;
  const bounds = drawer.getBoundingClientRect();
  if (event.clientX < bounds.left || event.clientX >= bounds.right ||
      event.clientY < bounds.top || event.clientY >= bounds.bottom) drawer.close();
}});
drawer.addEventListener('close', () => {{
  trigger.setAttribute('aria-expanded', 'false');
  if (!document.activeElement.closest('main')) trigger.focus({{preventScroll: true}});
}});
const relatedLinks = document.querySelectorAll('main .related-case');
for (const link of [...links, ...relatedLinks]) link.addEventListener('click', event => {{
  if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
  event.preventDefault();
  const target = document.getElementById(link.hash.slice(1));
  if (drawer.open) drawer.close();
  if (location.hash !== link.hash) location.hash = link.hash;
  selectScenario();
  target.querySelector('h2').focus({{preventScroll: true}});
}});
const groups = [...drawer.querySelectorAll('.feature-group')];
const search = drawer.querySelector('.drawer-search');
const filterImages = drawer.querySelector('.filter-images');
function filterScenarios() {{
  const query = search.value.trim().normalize('NFKC').toLowerCase();
  for (const group of groups) {{
    const feature = group.querySelector('h3').textContent;
    const cases = [...group.querySelectorAll('a')];
    for (const link of cases) {{
      const haystack = (feature + ' ' + link.textContent).normalize('NFKC').toLowerCase();
      link.hidden = !haystack.includes(query) || (filterImages.checked && link.dataset.images === '0');
    }}
    group.hidden = cases.every(link => link.hidden);
  }}
  drawer.querySelector('.no-matches').hidden = groups.some(group => !group.hidden);
}}
search.addEventListener('input', filterScenarios);
filterImages.addEventListener('change', filterScenarios);
addEventListener('pageshow', filterScenarios);
const main = document.querySelector('main');
const sections = [...main.children];
function selectScenario() {{
  const target = document.getElementById(location.hash.slice(1));
  const active = target?.closest('main > section');
  const current = active ?? document.getElementById(main.dataset.preferred) ?? sections[0];
  for (const link of links) {{
    if (link.hash === '#' + current.id) link.setAttribute('aria-current', 'location');
    else link.removeAttribute('aria-current');
  }}
  for (const section of sections) section.hidden = section !== current;
  if (active) requestAnimationFrame(() => target.scrollIntoView({{block: 'start'}}));
  else if (!location.hash) window.scrollTo(0, 0);
}}
addEventListener('hashchange', selectScenario);
selectScenario();
</script>
</body>
</html>
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("gherkin_zip", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--evidence", type=Path)
    parser.add_argument("--editor-evidence", type=Path)
    parser.add_argument("--pr", type=int, required=True)
    parser.add_argument("--sha", required=True)
    parser.add_argument("--run", type=int, required=True)
    args = parser.parse_args()
    require(args.pr > 0 and args.run > 0 and re.fullmatch(r"[0-9a-f]{40}", args.sha), "Invalid CI metadata")
    args.output.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(args.gherkin_zip) as archive:
        catalogue = read_catalog(archive)
    evidence = {}
    if args.evidence:
        with zipfile.ZipFile(args.evidence) as archive:
            evidence = read_report(archive, args.output, catalogue)
    if args.editor_evidence:
        editor_cases = editor_catalogue()
        with zipfile.ZipFile(args.editor_evidence) as archive:
            evidence.update(read_report(
                archive, args.output, [editor_cases],
                {key: value[1] for key, value in EDITOR_CASES.items()}, "editor-",
            ))
        catalogue.insert(0, editor_cases)
    markup = render(catalogue, evidence, args.pr, args.sha, args.run)
    (args.output / "index.html").write_text(markup, encoding="utf-8")


if __name__ == "__main__":
    main()
