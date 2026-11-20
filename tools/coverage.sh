#!/usr/bin/env bash
set -euo pipefail

repository=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
report_dir="$repository/artifacts/coverage"
fixture_dir=$(mktemp -d)
trap 'rm -rf -- "$fixture_dir"' EXIT

cd "$repository/rust"
mkdir -p "$report_dir"

cargo llvm-cov clean --workspace
cargo llvm-cov --locked --tests --no-report
cargo llvm-cov run --locked --bin meditor-fixture --no-report -- "$fixture_dir"
SDL_AUDIODRIVER=dummy xvfb-run -a timeout 55s \
    cargo llvm-cov run --locked --bin meditor --no-report -- --smoke-test "$fixture_dir/fixture.json"

report_scope='(^|/)(tests/|bin/meditor-fixture\.rs$)'
cargo llvm-cov report --ignore-filename-regex "$report_scope" \
    --json --summary-only --output-path "$report_dir/coverage.json"
cargo llvm-cov report --ignore-filename-regex "$report_scope" \
    --lcov --output-path "$report_dir/lcov.info"

line_counts=$(jq -ce '
    .data[0] as $coverage
    | [$coverage.files[] | select(.filename | test("(^|/)src/(lib|project)\\.rs$")) | .summary.lines] as $core
    | if ($core | length) != 2 then error("Coverage must include src/lib.rs and src/project.rs") else
        {
            app: $coverage.totals.lines,
            core: {
                covered: ($core | map(.covered) | add),
                count: ($core | map(.count) | add)
            }
        }
      end
' "$report_dir/coverage.json")

jq -r '
    def row($scope; $lines; $minimum):
        ($lines.covered * 10000 / $lines.count | round / 100) as $percent
        | "| \($scope) | \($lines.covered)/\($lines.count) | \($percent)% | \($minimum) |";
    "## Rust line coverage",
    "",
    "| Scope | Covered lines | Line coverage | Minimum |",
    "| --- | ---: | ---: | ---: |",
    row("Application"; .app; "Report only"),
    row("Playback core and project model"; .core; "85%")
' <<< "$line_counts" | tee "$report_dir/summary.md"

if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    cat "$report_dir/summary.md" >> "$GITHUB_STEP_SUMMARY"
fi

if ! jq -e '.core.covered * 100 >= .core.count * 85' <<< "$line_counts" >/dev/null; then
    echo "Playback core and project model line coverage fell below 85%." >&2
    exit 1
fi
