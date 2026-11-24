#!/usr/bin/env python3
"""Run implemented edit-history scenarios and optionally verify a known regression."""

import argparse
import json
import os
import shutil
import subprocess
from collections import Counter
from pathlib import Path
from tempfile import TemporaryDirectory

from gherkin.parser import Parser
from gherkin.pickles.compiler import Compiler

ROOT = Path(__file__).resolve().parent.parent
FEATURE = "docs/features/edit-history.feature"
# Counts include each expanded Examples row. Add coverage before growing this list.
PLANNED = {
    "保存したチャットと作業履歴から編集を再開する": 1,
    "GUI とチャットで同じ作業履歴を元に戻し、やり直す": 2,
    "後から行った独立した字幕修正を残してカット変更だけ差し戻す": 1,
    "差し戻しが後続の編集に影響する場合は確認を待つ": 2,
    "Redo の候補から外れた変更を作業履歴から確認する": 1,
}
REDO_SCENARIO = "Undo の後の新しい編集で通常の Redo の候補を消す"
REDO_ERROR = "Error: redo remains available after a new edit"


def case_label(case):
    return f"{FEATURE}:{case['location']['line']}: {case['name']}"


def scenarios():
    document = Parser().parse((ROOT / FEATURE).read_text(encoding="utf-8"))
    document["uri"] = FEATURE
    active = []
    planned = Counter()
    for case in Compiler().compile(document):
        name = case["name"]
        tags = {tag["name"] for tag in case["tags"]}
        if "@planned" in tags:
            planned[name] += 1
            print(f"PLANNED {case_label(case)}", flush=True)
        else:
            if not any(step["type"] == "Outcome" for step in case["steps"]):
                raise SystemExit(f"{case_label(case)}: scenario has no expected outcome")
            if any("argument" in step for step in case["steps"]):
                raise SystemExit(f"{case_label(case)}: step arguments have no runner support")
            active.append(case)
    if planned != PLANNED:
        raise SystemExit(f"Unexpected @planned scenarios: {dict(planned)}")
    if not active:
        raise SystemExit("No executable edit-history scenarios")
    return active


def build(project, target, example="history_bdd"):
    subprocess.run(
        [
            "cargo", "build", "--manifest-path", str(project / "Cargo.toml"),
            "--locked", "--release", "--example", example, "--target-dir", str(target),
        ],
        check=True,
    )
    executable = f"{example}.exe" if os.name == "nt" else example
    return target / "release" / "examples" / executable


def run_cases(binary, cases, *, mutated=False):
    detected = 0
    failures = []
    for case in cases:
        name = case["name"]
        result = subprocess.run(
            [binary],
            input=json.dumps([step["text"] for step in case["steps"]]),
            text=True,
            encoding="utf-8",
            capture_output=True,
            timeout=15,
        )
        label = case_label(case)
        if mutated and name == REDO_SCENARIO:
            if result.returncode == 1 and result.stderr.strip() == REDO_ERROR:
                print(f"DETECTED {label}", flush=True)
                detected += 1
                continue
            failures.append(f"{label}: expected {REDO_ERROR!r}, got {result.stderr.strip()!r}")
        elif result.returncode != 0:
            failures.append(f"{label}: {result.stderr.strip() or result.returncode}")
        else:
            print(f"PASS {label}", flush=True)
    if failures:
        raise SystemExit("\n".join(failures))
    if mutated and detected != 1:
        raise SystemExit(f"Expected to detect the redo regression once, detected {detected} times")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check-mutation", action="store_true")
    args = parser.parse_args()

    cases = scenarios()
    project = ROOT / "rust"
    target = project / "target"
    run_cases(build(project, target), cases)
    if args.check_mutation:
        with TemporaryDirectory(prefix="meditor-history-mutation-") as folder:
            mutated = Path(folder) / "rust"
            shutil.copytree(project, mutated, ignore=shutil.ignore_patterns("target"))
            history = mutated / "src/history.rs"
            source = history.read_text(encoding="utf-8")
            line = "            self.redo.clear();\n"
            if source.count(line) != 1:
                raise SystemExit("Could not locate the unique redo-clearing statement to mutate")
            history.write_text(source.replace(line, ""), encoding="utf-8")
            (mutated / "examples/history_bdd.rs").rename(
                mutated / "examples/history_bdd_mutation.rs"
            )
            run_cases(build(mutated, target, "history_bdd_mutation"), cases, mutated=True)


if __name__ == "__main__":
    main()
