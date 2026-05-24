import { existsSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { delimiter, join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

const startedAt = new Date();
const bun = resolveBun();

if (!bun) {
  writeMissingBunReport(startedAt, "command not found: bun");
  process.exit(127);
}

const env = {
  ...process.env,
  PATH: [
    join(homedir(), ".bun", "bin"),
    join(homedir(), ".cargo", "bin"),
    process.env.PATH ?? "",
  ].join(delimiter),
};

const result = spawnSync(bun, ["run", "scripts/selftest.ts"], {
  cwd: resolve("."),
  env,
  stdio: "inherit",
});

process.exit(result.status ?? 1);

function resolveBun() {
  const pathResult = spawnSync("bun", ["--version"], { encoding: "utf8" });
  if (pathResult.status === 0) {
    return "bun";
  }

  const homeBun = join(homedir(), ".bun", "bin", process.platform === "win32" ? "bun.exe" : "bun");
  if (existsSync(homeBun)) {
    return homeBun;
  }

  return undefined;
}

function writeMissingBunReport(startedAt, stderr) {
  const finishedAt = new Date();
  const step = {
    name: "check Bun runtime",
    status: "fail",
    durationMs: finishedAt.getTime() - startedAt.getTime(),
    command: "bun --version",
    exitCode: 127,
    stdout: "",
    stderr,
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
}
