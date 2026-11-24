import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";

const require = createRequire(import.meta.url);
const quint = require.resolve("@informalsystems/quint/dist/src/cli.js");
const dir = mkdtempSync(join(tmpdir(), "meditor-edit-history-"));
const fixtures = [
  ["history_1.itf.json", "edit_history.itf.json"],
  ["history_2.itf.json", "edit_history_full.itf.json"],
  ["reopen.itf.json", "edit_history_reopen.itf.json"],
];

function runTrace(options) {
  execFileSync(process.execPath, [
    quint, "run", "EditHistory.qnt", "--backend=typescript", "--invariant=historyRules",
    "--verbosity=1", ...options,
  ], { cwd: new URL(".", import.meta.url), stdio: "inherit" });
}

try {
  runTrace([
    "--seed=0x512", "--max-samples=64", "--n-traces=4", "--max-steps=24",
    "--out-itf=" + join(dir, "history_{seq}.itf.json"),
  ]);
  runTrace([
    "--step=checkpointStep", "--max-samples=1", "--max-steps=9",
    "--out-itf=" + join(dir, "reopen.itf.json"),
  ]);

  for (const [source, name] of fixtures) {
    const fixture = new URL("./traces/" + name, import.meta.url);
    const trace = JSON.parse(readFileSync(join(dir, source), "utf8"));
    delete trace["#meta"].description;
    delete trace["#meta"].timestamp;
    if (process.argv.includes("--write")) {
      const { states, ...header } = trace;
      const lines = states.map(state => "    " + JSON.stringify(state));
      writeFileSync(fixture, JSON.stringify(header, null, 2).slice(0, -2) +
        ',\n  "states": [\n' + lines.join(",\n") + "\n  ]\n}\n");
    } else {
      assert.deepEqual(trace, JSON.parse(readFileSync(fixture, "utf8")),
        `${name} differs. Regenerate with npm run check:history-trace -- --write.`);
    }
  }
} finally {
  rmSync(dir, { recursive: true, force: true });
}
