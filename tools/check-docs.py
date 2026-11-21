#!/usr/bin/env python3
"""Check repository documentation links and compile the Gherkin acceptance specs."""

import json
import re
import sys
from functools import cache
from pathlib import Path
from urllib.parse import unquote, urlsplit

from gherkin.dialect import Dialect
from gherkin.errors import CompositeParserException
from gherkin.parser import Parser
from gherkin.pickles.compiler import Compiler
from github_slugger import GithubSlugger
from markdown_it import MarkdownIt

ROOT = Path(__file__).resolve().parent.parent
FEATURES = ROOT / "docs/features"
MARKDOWN = MarkdownIt("commonmark")
PLACEHOLDER = re.compile(r"<[^\s<>]+>")


@cache
def markdown_tokens(path):
    return MARKDOWN.parse(path.read_text(encoding="utf-8"))


@cache
def heading_anchors(path):
    slugger = GithubSlugger()
    anchors = set()
    tokens = markdown_tokens(path)
    for index, token in enumerate(tokens):
        if token.type == "heading_open":
            heading = "".join(child.content for child in tokens[index + 1].children or [])
            anchors.add(slugger.slug(heading))
    return anchors


def local_links(path):
    for token in markdown_tokens(path):
        for child in token.children or []:
            if child.type == "link_open":
                href = child.attrGet("href")
            elif child.type == "image":
                href = child.attrGet("src")
            else:
                continue
            url = urlsplit(href)
            if url.scheme or url.netloc:
                continue
            target = unquote(url.path)
            if target.startswith("/"):
                target = ROOT / target.lstrip("/")
            else:
                target = path.parent / target if target else path
            yield href, target.resolve(), unquote(url.fragment), (token.map or [0])[0] + 1


def check_markdown(pages):
    errors = []
    for page in pages:
        for href, target, anchor, line in local_links(page):
            location = f"{page.relative_to(ROOT)}:{line}"
            if not target.is_relative_to(ROOT) or not target.exists():
                errors.append(f"{location}: broken repository link {href!r}")
            elif anchor and target.suffix.lower() == ".md" and anchor not in heading_anchors(target):
                errors.append(f"{location}: missing Markdown heading anchor in {href!r}")
    return errors


def scenario_nodes(children):
    for child in children:
        if "scenario" in child:
            yield child["scenario"]
        elif "rule" in child:
            yield from scenario_nodes(child["rule"]["children"])


def check_features(paths):
    errors = []
    count = 0
    indexed = {target for _, target, _, _ in local_links(FEATURES / "README.md")}
    for path in paths:
        relative = path.relative_to(ROOT).as_posix()
        for paired in (path, path.with_suffix(".md")):
            if paired not in indexed or not paired.is_file():
                errors.append(f"{relative}: missing feature or definition in docs/features/README.md")
                break
        try:
            document = Parser().parse(path.read_text(encoding="utf-8"))
        except CompositeParserException as error:
            errors.append(f"{relative}: {error}")
            continue
        document["uri"] = relative
        scenarios = Compiler().compile(document)
        if not scenarios:
            errors.append(f"{relative}: feature has no executable scenarios")
        compiled_ids = {node_id for case in scenarios for node_id in case["astNodeIds"]}
        feature = document.get("feature", {})
        outlines = Dialect.for_name(feature.get("language", "en")).scenario_outline_keywords
        for scenario in scenario_nodes(feature.get("children", [])):
            if not scenario["steps"]:
                errors.append(f"{relative}: scenario has no steps: {scenario['name']}")
            outline_without_examples = scenario["keyword"] in outlines and not scenario["examples"]
            if outline_without_examples or scenario["id"] not in compiled_ids:
                errors.append(f"{relative}: scenario has no runnable example: {scenario['name']}")
        for case in scenarios:
            if PLACEHOLDER.search(json.dumps(case, ensure_ascii=False)):
                errors.append(f"{relative}: unresolved Examples placeholder: {case['name']}")
        count += len(scenarios)
    return errors, count


def main():
    pages = [ROOT / "README.md", *sorted((ROOT / "docs").rglob("*.md"))]
    paths = sorted(FEATURES.rglob("*.feature"))
    errors = check_markdown(pages)
    feature_errors, count = check_features(paths)
    errors.extend(feature_errors)
    for error in errors:
        print(error, file=sys.stderr)
    if errors:
        return 1
    print(f"Checked {len(pages)} Markdown pages and {count} scenarios in {len(paths)} features.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
