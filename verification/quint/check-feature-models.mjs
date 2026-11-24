import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const quint = require.resolve("@informalsystems/quint/dist/src/cli.js");
const directory = new URL(".", import.meta.url);
const specs = readdirSync(directory)
  .filter(name => name.endsWith(".qnt") && /^\s*\/\/ BDD:/m.test(readFileSync(new URL(name, directory), "utf8")))
  .sort();

if (specs.length === 0) throw new Error("No BDD workflow models were found.");
for (const spec of specs) {
  const options = { cwd: directory, stdio: "inherit" };
  execFileSync(process.execPath, [quint, "test", spec, "--backend=typescript"], options);
  execFileSync(process.execPath, [quint, "verify", spec, "--backend=tlc", "--invariant=workflowRules"], options);
}
