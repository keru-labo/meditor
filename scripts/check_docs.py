"""Check repository Markdown links and the BDD contract's test mapping."""

from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
TEST = ROOT / "tests/latest_seek_test.cpp"
ACCEPTANCE = ROOT / "docs/acceptance.md"


def main():
    documents = [ROOT / "README.md", *sorted((ROOT / "docs").glob("*.md"))]
    for markdown in documents:
        text = markdown.read_text()
        for link in re.findall(r"\]\(([^\s)]+)\)", text):
            path = link.partition("#")[0]
            if path and "://" not in path and not (markdown.parent / path).exists():
                raise SystemExit(f"Broken link in {markdown.relative_to(ROOT)}: {link}")

    acceptance = ACCEPTANCE.read_text()
    if "| Scenario | Given | When | Then |" not in acceptance:
        raise SystemExit("Missing BDD table")
    rows = re.findall(r"^\| `([a-z-]+)` +(.*) \|$", acceptance, re.M)
    scenarios = {name for name, _ in rows}
    for name, cells in rows:
        columns = cells.split("|")
        if len(columns) != 4 or columns[0].strip() or not all(c.strip() for c in columns[1:]):
            raise SystemExit(f"Incomplete Given/When/Then: {name}")
    tests = set(re.findall(r', "([a-z-]+)"\);', TEST.read_text()))
    if not scenarios or len(rows) != len(scenarios) or scenarios != tests:
        raise SystemExit(f"BDD and tests differ: {sorted(scenarios ^ tests)}")
    print(f"Checked {len(documents)} Markdown files and {len(scenarios)} BDD scenarios")


if __name__ == "__main__":
    main()
