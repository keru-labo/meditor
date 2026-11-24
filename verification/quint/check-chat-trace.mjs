import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { reproduceFixture } from "./trace-fixture.mjs";

const require = createRequire(import.meta.url);
const quint = require.resolve("@informalsystems/quint/dist/src/cli.js");
const dir = mkdtempSync(join(tmpdir(), "meditor-chat-edit-"));
const output = join(dir, "cancelled.itf.json");
try {
  execFileSync(process.execPath, [
    quint, "run", "ChatEdit.qnt", "--backend=typescript",
    "--step=cancelledReplyTraceStep", "--invariant=pollPreservesEditsWhenRequired",
    "--max-samples=1", "--max-steps=5", "--verbosity=1", "--out-itf=" + output,
  ], { cwd: new URL(".", import.meta.url), stdio: "inherit" });
  reproduceFixture(output, "chat_edit_cancelled.itf.json", "npm run check:chat-trace");
} finally {
  rmSync(dir, { recursive: true, force: true });
}
