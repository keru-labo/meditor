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
            rendered.append(
                f"<li><h3>{action}</h3>"
                "<table><thead><tr><th>確認点</th><th>期待</th><th>実際</th><th>結果</th></tr></thead>"
                f"<tbody>{''.join(rows)}</tbody></table>"
                f"<a href='{name}'><img src='{name}' loading='lazy' alt='{action}後の画面'></a></li>"
            )
        require(failed or len(steps) == maximum, "Incomplete passing scenario")
        scenarios.append((text(scenario["name"]), failed, "".join(rendered)))
    require(used == files.keys(), "Unexpected screenshot")
    return scenarios


def render(scenarios, pr, sha, run):
    scenarios = sorted(scenarios, key=lambda scenario: not scenario[1])
    tabs = []
    panels = []
    for index, (name, failed, steps) in enumerate(scenarios, 1):
        status = "失敗" if failed else "成功"
        state = "fail" if failed else "pass"
        tabs.append(f"<a href='#scenario-{index}'><span class='{state}'>{status}</span> {name}</a>")
        panels.append(
            f"<section id='scenario-{index}'><h2>{name}</h2><ol>{steps}</ol></section>"
        )
    return f"""<!doctype html>
<html lang="ja"><meta charset="utf-8">
<meta name="robots" content="noindex, nofollow, noarchive">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Undo / Redo の画面検査 · PR #{pr}</title>
<style>
*{{box-sizing:border-box}}
:root{{color:#e8edf7;background:#0d1421;font:16px/1.6 system-ui,sans-serif;color-scheme:dark}}
body{{margin:0 auto;max-width:88rem;padding:clamp(1rem,4vw,3rem)}}
a{{color:#9dd9ff}} a:focus-visible{{outline:2px solid #9dd9ff;outline-offset:3px}}
h1,h2,h3{{line-height:1.35}} h1{{margin:.4rem 0}}
header p{{margin:0 0 1.75rem;color:#b5c4db}}
.layout{{display:grid;grid-template-columns:minmax(12rem,19rem) minmax(0,1fr);gap:1.5rem}}
nav{{align-self:start;position:sticky;top:1rem}}
nav a{{display:block;margin:.5rem 0;padding:.85rem;background:#172335;border-radius:.6rem;text-decoration:none}}
nav a[aria-current]{{box-shadow:inset 0 0 0 2px #77b7ef}}
section{{min-width:0}} section h2{{margin-top:0}}
ol{{margin:0;padding-left:1.5rem}} li{{padding:.9rem;margin:.75rem 0;background:#172335;border-radius:.65rem}}
h3{{margin:0 0 .65rem}}
table{{width:100%;border-collapse:collapse;text-align:left;margin-bottom:1rem}}
th,td{{padding:.35rem .5rem;border-bottom:1px solid #3c4b62;overflow-wrap:anywhere}}
.pass{{color:#8ee3b1}} .fail{{color:#ffa69e}}
img{{display:block;max-width:100%;height:auto;border:1px solid #3c4b62;border-radius:.4rem}}
@media (max-width:650px){{.layout{{grid-template-columns:1fr}} nav{{position:static}}}}
</style>
<header><h1>Undo / Redo の画面検査</h1><p>
<a href="https://github.com/keru-labo/meditor/pull/{pr}">PR #{pr}</a> · <code>{sha[:12]}</code> ·
<a href="https://github.com/keru-labo/meditor/actions/runs/{run}">CIの実行結果</a></p></header>
<div class="layout"><nav aria-label="シナリオ">{"".join(tabs)}</nav>
<main>{"".join(panels)}</main></div>
<script>
const links = [...document.querySelectorAll('nav a')];
const sections = [...document.querySelectorAll('main section')];
function selectScenario() {{
  const current = links.find(link => link.hash === location.hash) || links[0];
  for (const link of links) {{
    if (link === current) link.setAttribute('aria-current', 'true');
    else link.removeAttribute('aria-current');
  }}
  for (const section of sections) section.hidden = '#' + section.id !== current.hash;
}}
addEventListener('hashchange', selectScenario);
selectScenario();
</script>
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
