#!/usr/bin/env python3
"""Collect raw Gherkin for CI and compile the static report catalog."""

import argparse
import hashlib
import json
import re
import stat
from pathlib import Path

FEATURE_ROOT = Path(__file__).resolve().parent.parent / "docs/features"
CLIP_FEATURE = "docs/features/ai-clip-editing.feature"
INVALID_CLIP_EXAMPLES = {
    ("開始", "-1"): "ui-clip-invalid-start-negative",
    ("終了", "91"): "ui-clip-invalid-end-outside",
    ("開始", "40"): "ui-clip-invalid-start-equal",
    ("終了", "31"): "ui-clip-invalid-end-before",
}
EVIDENCE_FEATURES = {
    "ui-redo-after-edit": "docs/features/edit-history.feature",
    "ui-reopen-history": "docs/features/edit-history.feature",
    "ui-clip-trim-start": CLIP_FEATURE,
    "ui-clip-trim-end": CLIP_FEATURE,
    "ui-subtitle-style": "docs/features/subtitle-editing.feature",
    **dict.fromkeys(INVALID_CLIP_EXAMPLES.values(), CLIP_FEATURE),
}
MAX_FILES = 64
MAX_ARCHIVE_ENTRIES = 160
MAX_FILE_BYTES = 64 * 1024
MAX_SOURCE_BYTES = 512 * 1024
MAX_CASES = 512
MAX_STEPS = 8192
MAX_TAGS = 16384
MAX_TEXT = 8192
MAX_EXPANDED_BYTES = 4 * 1024 * 1024
MAX_MATCH_WORK = 32 * 1024 * 1024


def require(condition, message):
    if not condition:
        raise ValueError(message)


def is_feature_path(path):
    parts = path.split("/")
    return valid_parts(parts) and len(parts) >= 3 and parts[-1].endswith(".feature")


def valid_parts(parts):
    return (
        parts[:2] == ["docs", "features"]
        and sum(len(part) + 1 for part in parts) <= 240
        and all(
            part not in ("", ".", "..")
            and not any(character in part for character in "\\:#?")
            and not any(ord(character) < 32 or ord(character) == 127 for character in part)
            for part in parts
        )
    )


def collect_feature_sources(destination, root=FEATURE_ROOT):
    """Stage only tracked-style regular feature files, without parsing PR code."""
    require(root.is_dir() and root.resolve() == root, "Unsafe Gherkin source directory")
    files = sorted(root.rglob("*.feature"))
    require(0 < len(files) <= MAX_FILES, "Unexpected number of Gherkin files")
    total = 0
    for source in files:
        name = f"docs/features/{source.relative_to(root).as_posix()}"
        require(is_feature_path(name) and source.is_file() and source.resolve() == source,
                "Unsafe Gherkin source file")
        size = source.stat().st_size
        require(size <= MAX_FILE_BYTES, f"{name}: Gherkin source is too large")
        total += size
        require(total <= MAX_SOURCE_BYTES, "Gherkin sources are too large")
        target = destination / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(source.read_bytes())


def feature_members(archive):
    entries = archive.infolist()
    require(len(entries) <= MAX_ARCHIVE_ENTRIES, "Too many Gherkin archive entries")
    seen = set()
    files = []
    total = 0
    for entry in entries:
        name = entry.filename
        mode = stat.S_IFMT(entry.external_attr >> 16)
        require(name == entry.orig_filename and name not in seen, "Duplicate or invalid Gherkin archive entry")
        seen.add(name)
        if entry.is_dir():
            require(mode in (0, stat.S_IFDIR) and entry.file_size == 0
                    and (name == "docs/" or valid_parts(name[:-1].split("/"))),
                    "Unexpected Gherkin archive directory")
            continue
        require(is_feature_path(name) and mode in (0, stat.S_IFREG), "Unexpected Gherkin archive file")
        require(entry.file_size <= MAX_FILE_BYTES, f"{name}: Gherkin source is too large")
        total += entry.file_size
        require(total <= MAX_SOURCE_BYTES, "Gherkin sources are too large")
        files.append(entry)
    require(0 < len(files) <= MAX_FILES, "Unexpected number of Gherkin files")
    return sorted(files, key=lambda entry: entry.filename)


def bounded(value):
    require(len(value) <= MAX_TEXT, "Expanded Gherkin text is too long")
    return value


def index_feature(feature, outlines, uri):
    scenarios = {}
    steps = {}
    examples = {}
    case_count = step_count = tag_count = 0

    def walk(children, rule=None, background_count=0, inherited_tags=0):
        nonlocal case_count, step_count, tag_count
        for child in children:
            if "background" in child:
                background = child["background"]["steps"]
                background_count += len(background)
                steps.update((step["id"], (step, True)) for step in background)
            elif "rule" in child:
                nested = child["rule"]
                walk(nested["children"], nested["name"], background_count, inherited_tags + len(nested["tags"]))
            else:
                scenario = child["scenario"]
                authored_steps = scenario["steps"]
                require(authored_steps, f"{uri}:{scenario['location']['line']}: scenario has no authored steps")
                steps.update((step["id"], (step, False)) for step in authored_steps)
                scenarios[scenario["id"]] = (scenario, rule)
                example_count = 0
                for example in scenario["examples"]:
                    header = example.get("tableHeader", {}).get("cells", [])
                    columns = [bounded(cell["value"]) for cell in header]
                    require(len(set(columns)) == len(columns), f"{uri}: duplicate Gherkin Examples column")
                    for row in example.get("tableBody", []):
                        values = [bounded(cell["value"]) for cell in row["cells"]]
                        require(len(columns) == len(values), f"{uri}: mismatched Gherkin Examples row")
                        pairs = list(zip(columns, values, strict=True))
                        examples[row["id"]] = dict(line=row["location"]["line"], values=pairs)
                        example_count += 1
                        tag_count += len(example["tags"])
                require(example_count or (not scenario["examples"] and scenario["keyword"] not in outlines),
                        f"{uri}:{scenario['location']['line']}: scenario has no runnable Examples row")
                count = example_count or 1
                case_count += count
                step_count += count * (background_count + len(authored_steps))
                tag_count += count * (inherited_tags + len(scenario["tags"]))

    walk(feature["children"], inherited_tags=len(feature["tags"]))
    return scenarios, steps, examples, case_count, step_count, tag_count


def capture_id(tags, evidence_ids, uri, example):
    bindings = {tag[1:] for tag in tags if tag[1:] in EVIDENCE_FEATURES and tag[1:] not in INVALID_CLIP_EXAMPLES.values()}
    if "@ui-clip-invalid" in tags:
        values = dict(example["values"]) if example else {}
        invalid_clip_id = INVALID_CLIP_EXAMPLES.get((values.get("境界"), values.get("秒")))
        require(invalid_clip_id is not None, f"{uri}: unexpected invalid clip example")
        bindings.add(invalid_clip_id)
    require(len(bindings) <= 1, f"{uri}: Gherkin case has conflicting UI evidence tags")
    binding = next(iter(bindings), None)
    if binding is not None:
        require(uri == EVIDENCE_FEATURES[binding] and "@planned" not in tags, f"{uri}: invalid UI evidence tag @{binding}")
        require(binding not in evidence_ids, f"{uri}: duplicate UI evidence tag @{binding}")
        evidence_ids.add(binding)
    return binding


def step_argument(argument):
    if "dataTable" in argument:
        return dict(kind="table", rows=[
            [bounded(cell["value"]) for cell in row["cells"]]
            for row in argument["dataTable"]["rows"]
        ])
    if "docString" in argument:
        doc = argument["docString"]
        return dict(kind="doc_string", content=bounded(doc["content"]), media_type=bounded(doc.get("mediaType", "")))
    return None


def read_catalog(archive):
    """Read an untrusted ZIP as data and expand cases with the pinned Gherkin library."""
    from gherkin.dialect import Dialect
    from gherkin.errors import CompositeParserException
    from gherkin.parser import Parser
    from gherkin.pickles.compiler import Compiler

    compiled_bytes = 0
    rendered_bytes = 0
    match_work = 0

    class LiteralCompiler(Compiler):
        def __init__(self):
            super().__init__()
            self.previous_columns = None
            self.previous_row = None
            self.values = {}

        # The pinned compiler interpolates Examples headers as regexes. Substitute
        # exact tokens so PR-provided headers cannot become regex programs.
        def _interpolate(self, name, variable_cells, value_cells):
            nonlocal compiled_bytes, match_work
            if name is None:
                return None
            bounded(name)
            if value_cells:
                if variable_cells is not self.previous_columns:
                    self.previous_columns = variable_cells
                    self.columns = [f"<{cell['value']}>" for cell in variable_cells]
                    longest_first = sorted(self.columns, key=len, reverse=True)
                    self.placeholders = re.compile("|".join(re.escape(column) for column in longest_first))
                    self.match_cost = sum(map(len, self.columns))
                if value_cells is not self.previous_row:
                    self.previous_row = value_cells
                    self.values = {
                        column: cell["value"]
                        for column, cell in zip(self.columns, value_cells, strict=True)
                    }
                # Bound literal comparisons even when no candidate matches.
                match_work += name.count("<") * self.match_cost
                require(match_work <= MAX_MATCH_WORK, "Gherkin Examples matching exceeds the report limits")
                fragments = []
                position = length = 0
                for match in self.placeholders.finditer(name):
                    value = self.values[match[0]]
                    length += match.start() - position + len(value)
                    require(length <= MAX_TEXT, "Expanded Gherkin text is too long")
                    fragments.extend((name[position:match.start()], value))
                    position = match.end()
                fragments.append(name[position:])
                require(length + len(name) - position <= MAX_TEXT, "Expanded Gherkin text is too long")
                name = "".join(fragments)
            # An empty table cell still allocates a pickle cell and a report element.
            compiled_bytes += len(name.encode("utf-8")) + 32
            require(compiled_bytes <= MAX_EXPANDED_BYTES, "Expanded Gherkin arguments are too large")
            return name

    catalog = []
    evidence_ids = set()
    total_cases = total_steps = total_tags = 0
    for entry in feature_members(archive):
        uri = entry.filename
        try:
            with archive.open(entry) as source:
                raw = source.read(MAX_FILE_BYTES + 1)
            require(len(raw) <= MAX_FILE_BYTES, f"{uri}: Gherkin source is too large")
            document = Parser().parse(raw.decode("utf-8"))
        except (CompositeParserException, UnicodeError) as error:
            raise ValueError(f"{uri}: invalid Gherkin source") from error
        feature = document.get("feature")
        require(feature, f"{uri}: no Feature found")
        outlines = Dialect.for_name(feature["language"]).scenario_outline_keywords
        scenarios, steps, examples, case_count, step_count, tag_count = index_feature(feature, outlines, uri)
        total_cases += case_count
        total_steps += step_count
        total_tags += tag_count
        require(case_count > 0 and total_cases <= MAX_CASES and total_steps <= MAX_STEPS and total_tags <= MAX_TAGS,
                f"{uri}: Gherkin expansion exceeds the report limits")

        document["uri"] = uri
        cases = LiteralCompiler().compile(document)
        require(len(cases) == case_count, f"{uri}: Gherkin expansion does not match authored cases")
        rendered_cases = []
        for case in cases:
            scenario, rule = scenarios[case["astNodeIds"][0]]
            line = case["location"]["line"]
            tags = list(dict.fromkeys(bounded(tag["name"]) for tag in case["tags"]))
            rendered_steps = []
            for step in case["steps"]:
                authored, background = steps[step["astNodeIds"][0]]
                rendered_steps.append(dict(
                    keyword=bounded(authored["keyword"].strip()), text=bounded(step["text"]),
                    kind=step.get("type") or "Unknown", line=authored["location"]["line"],
                    background=background, argument=step_argument(step.get("argument", {})),
                ))
            case_id = hashlib.sha256(f"{uri}:{line}".encode()).hexdigest()[:24]
            example = examples.get(case["astNodeIds"][-1])
            rendered = dict(
                id=case_id, name=bounded(case["name"]), rule=bounded(rule) if rule else None,
                line=line, tags=tags, planned="@planned" in tags, evidence_id=capture_id(tags, evidence_ids, uri, example),
                steps=rendered_steps, example=example,
                description=bounded(scenario["description"].strip()),
            )
            rendered_bytes += len(json.dumps(rendered, ensure_ascii=False).encode("utf-8"))
            require(rendered_bytes <= MAX_EXPANDED_BYTES, "Expanded Gherkin catalog is too large")
            rendered_cases.append(rendered)
        catalog.append(dict(
            name=bounded(feature["name"]), uri=uri, line=feature["location"]["line"],
            description=bounded(feature["description"].strip()), scenarios=rendered_cases,
        ))
    return catalog


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    collect_feature_sources(args.output)


if __name__ == "__main__":
    main()
