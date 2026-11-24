import assert from "node:assert/strict";
import { readFileSync, writeFileSync } from "node:fs";

export function reproduceFixture(source, name, command) {
  const fixture = new URL("./traces/" + name, import.meta.url);
  const trace = JSON.parse(readFileSync(source, "utf8"));
  delete trace["#meta"].description;
  delete trace["#meta"].timestamp;
  if (process.argv.includes("--write")) {
    const { states, ...header } = trace;
    const lines = states.map(state => "    " + JSON.stringify(state));
    writeFileSync(fixture, JSON.stringify(header, null, 2).slice(0, -2) +
      ',\n  "states": [\n' + lines.join(",\n") + "\n  ]\n}\n");
  } else {
    assert.deepEqual(trace, JSON.parse(readFileSync(fixture, "utf8")),
      `${name} differs. Regenerate with ${command} -- --write.`);
  }
}
