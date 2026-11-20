"""Run the always-on contract, line coverage, and targeted mutations with GCC."""

import gzip
import json
import os
from pathlib import Path
import subprocess
from tempfile import TemporaryDirectory

ROOT = Path(__file__).resolve().parents[1]
HEADER = ROOT / "src/LatestSeek.h"
TEST = ROOT / "tests/latest_seek_test.cpp"
MUTATIONS = {
    "first request wins": ("pending_ = std::clamp", "if (!pending_) pending_ = std::clamp"),
    "unbounded request": (
        "std::clamp(position, Position{0}, std::max(Position{0}, duration))",
        "position",
    ),
    "take leaves request pending": ("std::exchange(pending_, std::nullopt)", "pending_"),
    "clear leaves request pending": ("pending_.reset();", ""),
}


def compile_test(directory, coverage=False):
    binary = directory / "seek"
    flags = ["--coverage"] if coverage else []
    subprocess.run(
        [os.environ.get("CXX", "g++"), "-std=c++17", *flags,
         "-I", str(directory), str(TEST), "-o", str(binary)],
        cwd=directory, check=True,
    )
    return binary


def check_coverage(directory):
    subprocess.run(
        ["gcov", "--json-format", "seek-latest_seek_test.gcno"],
        cwd=directory, check=True, stdout=subprocess.DEVNULL,
    )
    with gzip.open(next(directory.glob("*.gcov.json.gz")), "rt") as report:
        files = json.load(report)["files"]
    source, = [file for file in files if Path(file["file"]).name == HEADER.name]
    lines = source["lines"]
    covered = sum(line["count"] > 0 for line in lines)
    percentage = 100 * covered / len(lines)
    print(f"{HEADER.name} line coverage: {percentage:.1f}% ({covered}/{len(lines)})")
    if percentage < 85:
        raise SystemExit("Seek policy coverage below 85%")


def main():
    source = HEADER.read_text()
    with TemporaryDirectory() as temp:
        directory = Path(temp)
        header = directory / HEADER.name
        header.write_text(source)
        subprocess.run([compile_test(directory, coverage=True)], check=True)
        check_coverage(directory)

        for name, (old, new) in MUTATIONS.items():
            if source.count(old) != 1:
                raise SystemExit(f"Update mutation: {name}")
            header.write_text(source.replace(old, new))
            result = subprocess.run([compile_test(directory)], capture_output=True, text=True)
            if result.returncode != 1 or not result.stderr.startswith("Failed: "):
                raise SystemExit(f"Mutation did not trigger a contract failure: {name}")
            print(f"Killed {name} ({result.stderr.strip()})")


if __name__ == "__main__":
    main()
