#!/usr/bin/env python3
"""Build the public history report from a CI ZIP containing data and screenshots."""

import argparse
import html
import json
import re
import struct
import zipfile
from pathlib import Path

PNG_HEADER = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR"
STEP_COUNTS = (3, 4)
IMAGE_LIMIT = 2 * 1024 * 1024


def require(condition, message):
    if not condition:
        raise ValueError(message)


def text(value):
    require(isinstance(value, str) and len(value) <= 512, "Invalid report text")
    return html.escape(value)


def read_report(archive, destination):
    files = {file.filename: file for file in archive.infolist()}
    require(
        len(files) == len(archive.infolist())
        and all(re.fullmatch(r"report\.json|[12]-[1-4]\.png", name) for name in files),
        "Unexpected report files",
    )
    require(files["report.json"].file_size <= 64 * 1024, "Report JSON is too large")
    results = json.loads(archive.read("report.json"))["scenarios"]
    require(isinstance(results, list) and len(results) == len(STEP_COUNTS), "Invalid scenarios")
    scenarios = []
    used = {"report.json"}
    for index, (scenario, maximum) in enumerate(zip(results, STEP_COUNTS, strict=True), 1):
        steps = scenario["steps"]
        require(isinstance(steps, list) and 0 < len(steps) <= maximum, "Invalid steps")
        rendered = []
        failed = False
        for position, step in enumerate(steps, 1):
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
            name = f"{index}-{position}.png"
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
            step_passed = all(check["expected"] == check["actual"] for check in checks)
            rationale = text(step.get("rationale", "画面から取得した値を、操作後に期待する値と照合しています。"))
            verdict = "なぜ成功か" if step_passed else "何が期待と違うか"
            result = "すべての確認点が期待値と一致しました。" if step_passed else "期待値と一致しなかった確認点があります。"
            rendered.append(
                f"<div class='action'><h3>{action}</h3>{instructions}</div>"
                "<figure><figcaption><span>操作直後の画面</span>"
                f"<a href='{name}' target='_blank' rel='noopener'>原寸で開く ↗</a></figcaption>"
                f"<img src='{name}' width='{width}' height='{height}' loading='lazy' alt='{action}直後のエディター画面'></figure>"
                f"<div class='checks'><h4 class='{'pass' if step_passed else 'fail'}'>{verdict}</h4>"
                f"<p>{result} {rationale}</p><table><thead><tr>"
                "<th scope='col'>確認点</th><th scope='col'>期待値</th><th scope='col'>実際の値</th>"
                f"<th scope='col'>結果</th></tr></thead><tbody>{''.join(rows)}</tbody></table></div>",
            )
        require(failed or len(steps) == maximum, "Incomplete passing scenario")
        scenarios.append((text(scenario["name"]), failed, rendered))
    require(used == files.keys(), "Unexpected screenshot")
    return scenarios


def render(scenarios, pr, sha, run):
    scenarios = sorted(enumerate(scenarios, 1), key=lambda scenario: not scenario[1][1])
    links = []
    panels = []
    for index, (name, failed, steps) in scenarios:
        scenario_id = f"scenario-{index}"
        status = "失敗" if failed else "成功"
        state = "fail" if failed else "pass"
        links.append(
            f"<a href='#{scenario_id}'><span>{name}</span>"
            f"<small class='{state}'>{status} · {len(steps)} ステップ</small></a>"
        )
        cards = []
        for position, content in enumerate(steps, 1):
            step_id = f"{scenario_id}-step-{position}"
            cards.append(
                f"<li class='step' id='{step_id}'>"
                f"<span class='step-number' aria-hidden='true'>{position}</span>{content}</li>"
            )
        panels.append(
            f"<section id='{scenario_id}' aria-labelledby='{scenario_id}-title'>"
            f"<h2 id='{scenario_id}-title'>{name}</h2>"
            f"<ol class='steps' aria-label='操作と結果'>{"".join(cards)}</ol></section>"
        )
    return f"""<!doctype html>
<html lang="ja">
<head>
<meta charset="utf-8">
<meta name="robots" content="noindex, nofollow, noarchive">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Undo / Redo の画面検査 · PR #{pr}</title>
<style>
* {{ box-sizing: border-box }}
:root {{
  color: #172540; background: #f4f6fa; font: 16px/1.65 system-ui, sans-serif;
  --line: #dbe3ee; --muted: #455572; --accent: #174ea5;
}}
[hidden] {{ display: none !important }}
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
.layout {{
  display: grid; grid-template-columns: minmax(13rem, 18rem) minmax(0, 1fr); gap: clamp(1rem, 3vw, 2.5rem);
  margin: auto; max-width: 94rem; padding: clamp(1rem, 4vw, 3rem);
}}
.scenarios {{ align-self: start; position: sticky; top: 1.25rem }}
.scenarios h2 {{ color: var(--muted); font-size: .92rem; margin-bottom: .65rem }}
.scenarios a {{
  display: block; padding: .85rem .95rem; margin-bottom: .5rem; border: 1px solid var(--line);
  border-radius: .6rem; background: #fff; color: inherit; text-decoration: none;
}}
.scenarios a:hover {{ background: #f1f6ff }}
.scenarios a[aria-current] {{ background: #edf4ff; border-color: #4780d1; box-shadow: inset 3px 0 #2563bd }}
.scenarios a span {{ display: block; font-weight: 650 }}
.scenarios small {{ font-size: .81rem }}
.pass {{ color: #167246 }} .fail {{ color: #a52830; font-weight: 650 }}
main, main section {{ min-width: 0 }}
main section {{ scroll-margin-top: 1rem }}
main h2 {{ font-size: clamp(1.35rem, 3vw, 2rem) }}
.steps {{
  position: relative; padding: 0 0 0 3rem; list-style: none;
  display: grid; gap: 1.5rem; margin: 1.5rem 0 0;
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
  .layout {{ grid-template-columns: 1fr }}
  .scenarios {{ position: static }}
  th, td {{ padding: .35rem }}
}}
</style>
</head>
<body>
<header class="topbar"><div class="topbar-inner"><h1>Undo / Redo の画面検査</h1><p>
<a href="https://github.com/keru-labo/meditor/pull/{pr}">PR #{pr}</a> · <code>{sha[:12]}</code> ·
<a href="https://github.com/keru-labo/meditor/actions/runs/{run}">CIの実行結果</a></p></div></header>
<div class="layout">
<nav class="scenarios" aria-labelledby="scenarios-title"><h2 id="scenarios-title">シナリオ</h2>{"".join(links)}</nav>
<main>{"".join(panels)}</main>
</div>
<script>
const links = [...document.querySelectorAll('.scenarios a')];
const sections = [...document.querySelectorAll('main section')];
function selectScenario() {{
  const target = document.getElementById(location.hash.slice(1));
  const current = target?.closest('main section') ?? sections[0];
  for (const link of links) {{
    if (link.hash === '#' + current.id) link.setAttribute('aria-current', 'location');
    else link.removeAttribute('aria-current');
  }}
  for (const section of sections) section.hidden = section !== current;
  if (target) requestAnimationFrame(() => target.scrollIntoView({{block: 'start'}}));
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
    parser.add_argument("zip", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--pr", type=int, required=True)
    parser.add_argument("--sha", required=True)
    parser.add_argument("--run", type=int, required=True)
    args = parser.parse_args()
    require(args.pr > 0 and args.run > 0 and re.fullmatch(r"[0-9a-f]{40}", args.sha), "Invalid CI metadata")
    args.output.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(args.zip) as archive:
        scenarios = read_report(archive, args.output)
    (args.output / "index.html").write_text(render(scenarios, args.pr, args.sha, args.run), encoding="utf-8")


if __name__ == "__main__":
    main()
