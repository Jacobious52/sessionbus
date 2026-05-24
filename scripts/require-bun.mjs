import { spawnSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import { resolve } from "node:path";

const startedAt = new Date();
const result = spawnSync("bun", ["--version"], { encoding: "utf8" });

if (result.status === 0) {
  process.exit(0);
}

const finishedAt = new Date();
const step = {
  name: "check Bun runtime",
  status: "fail",
  durationMs: finishedAt.getTime() - startedAt.getTime(),
  command: "bun --version",
  exitCode: result.status ?? 127,
  stdout: result.stdout ?? "",
  stderr: result.stderr || result.error?.message || "command not found: bun",
  likelyCause: "Bun is not installed or not on PATH.",
  suggestedNextAction:
    "Run the bun install command from https://bun.sh, then rerun `npm run selftest` or `bun run selftest`.",
};

const report = {
  status: "fail",
  startedAt: startedAt.toISOString(),
  finishedAt: finishedAt.toISOString(),
  durationMs: step.durationMs,
  summary: 'Selftest failed at "check Bun runtime".',
  failedStep: step,
  steps: [step],
};

writeFileSync(resolve("selftest-report.json"), `${JSON.stringify(report, null, 2)}\n`);
console.error(step.likelyCause);
console.error(step.suggestedNextAction);
process.exit(step.exitCode);
