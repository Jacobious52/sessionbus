#!/usr/bin/env bun
import { existsSync } from "node:fs";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  assertExcludes,
  assertIncludes,
  buildReport,
  commandDisplay,
  failedStepFromError,
  redactExcerpt,
  type StepResult,
  summarizeFailure,
} from "./selftest-lib.ts";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const reportPath = join(root, "selftest-report.json");
const startedAt = new Date();
const steps: StepResult[] = [];
let daemon: Bun.Subprocess | undefined;
let tempRoot: string | undefined;

type RunOptions = {
  cwd?: string;
  env?: Record<string, string>;
  allowFailure?: boolean;
};

async function main() {
  try {
    await checked("check Bun runtime", "bun", ["--version"]);
    await checked("check npm runtime", "npm", ["--version"]);
    await checked("build TypeScript workspace", "npm", ["run", "build"]);
    await checked("test TypeScript workspace", "npm", ["test"]);
    await checked("check Rust toolchain", "cargo", ["--version"]);
    await checked("format Rust workspace", "cargo", ["fmt", "--check"]);
    await checked("test Rust workspace", "cargo", ["test"]);
    await checked("build Rust workspace", "cargo", ["build", "--workspace"]);
    await runCliE2e();
  } catch (error) {
    if (!steps.some((step) => step.status === "fail")) {
      steps.push(failedStepFromError("selftest", error, 0));
    }
  } finally {
    if (daemon) {
      daemon.kill();
      await daemon.exited.catch(() => undefined);
    }
    if (tempRoot && !process.env.SESSIONBUS_SELFTEST_KEEP_TEMP) {
      await rm(tempRoot, { recursive: true, force: true });
    }

    const finishedAt = new Date();
    const report = buildReport(steps, startedAt, finishedAt);
    await Bun.write(reportPath, `${JSON.stringify(report, null, 2)}\n`);
    console.log(report.summary);
    console.log(`Report: ${reportPath}`);
    process.exitCode = report.status === "pass" ? 0 : 1;
  }
}

async function runCliE2e() {
  const aictx = join(root, "target", "debug", executableName("aictx"));
  const acpBridge = join(root, "target", "debug", executableName("sessionbus-acp-bridge"));
  if (!existsSync(aictx)) {
    throw new Error(`missing built binary: ${aictx}`);
  }
  if (!existsSync(acpBridge)) {
    throw new Error(`missing built binary: ${acpBridge}`);
  }

  tempRoot = await mkdtemp(join(tmpdir(), "sessionbus-selftest-"));
  const home = join(tempRoot, "home");
  await mkdir(home, { recursive: true });
  const dbPath = join(tempRoot, "sessionbus.db");
  const workspace = join(tempRoot, "workspace");
  await mkdir(workspace, { recursive: true });
  const servicePath = join(workspace, "service.yaml");
  await writeFile(servicePath, "name: api\nTOKEN=super-secret\n", "utf8");

  let port = 0;
  await manualStep("allocate daemon port", async () => {
    port = await pickPort();
  });
  const api = `http://127.0.0.1:${port}`;
  const baseEnv = {
    ...process.env,
    HOME: home,
    SESSIONBUS_DB: dbPath,
    SESSIONBUS_URL: api,
  };

  daemon = Bun.spawn({
    cmd: [aictx, "daemon", "--bind", `127.0.0.1:${port}`, "--db", dbPath],
    cwd: root,
    env: baseEnv,
    stdout: "pipe",
    stderr: "pipe",
  });

  await manualStep("wait for daemon health", async () => {
    await waitForHealth(api, 8_000);
  });

  const status = await checked("check daemon status through CLI", aictx, [
    "--api",
    api,
    "status",
  ], { env: baseEnv, cwd: workspace });
  await manualStep("verify daemon status output", async () => {
    assertIncludes(status.stdout, "sessionbus-daemon", "status service");
    assertIncludes(status.stdout, api, "status api");
  });

  const start = await checked("create session through CLI", aictx, [
    "--api",
    api,
    "start",
    "Selftest continuity",
    "--summary",
    "Verify durable AI workflow continuity.",
  ], { env: baseEnv, cwd: workspace });
  const sessionId = start.stdout.trim().split(/\s+/).at(-1);
  if (!sessionId?.startsWith("ses_")) {
    throw new Error(`expected session id from aictx start, got: ${start.stdout}`);
  }

  await checked("add note through CLI", aictx, [
    "--api",
    api,
    "note",
    "Issue only happens in staging",
  ], { env: baseEnv, cwd: workspace });

  await checked("add file snapshot through CLI", aictx, [
    "--api",
    api,
    "add-file",
    servicePath,
  ], { env: baseEnv, cwd: workspace });

  await checked("add decision through CLI", aictx, [
    "--api",
    api,
    "decision",
    "Start from staging config",
  ], { env: baseEnv, cwd: workspace });

  const show = await checked("show current session through CLI", aictx, [
    "--api",
    api,
    "show",
  ], { env: baseEnv, cwd: workspace });
  await manualStep("verify session show output", async () => {
    assertIncludes(show.stdout, "Selftest continuity", "show title");
    assertIncludes(show.stdout, "service.yaml", "show file artifact");
    assertIncludes(show.stdout, "Issue only happens in staging", "show note artifact");
    assertIncludes(show.stdout, "Start from staging config", "show decision");
  });

  const pack = await checked("pack session through CLI", aictx, [
    "--api",
    api,
    "pack",
    "--for",
    "chatgpt",
  ], { env: baseEnv, cwd: workspace });

  await manualStep("verify deterministic pack content", async () => {
    assertIncludes(pack.stdout, "Selftest continuity", "pack title");
    assertIncludes(pack.stdout, "Issue only happens in staging", "pack note");
    assertIncludes(pack.stdout, "service.yaml", "pack file artifact");
    assertIncludes(pack.stdout, "Start from staging config", "pack decision");
    assertIncludes(pack.stdout, "TOKEN=[REDACTED]", "pack redaction");
    assertExcludes(pack.stdout, "super-secret", "pack secret leakage");
  });

  await checked("register ACP bridge", acpBridge, ["--api", api, "register"], {
    env: baseEnv,
    cwd: workspace,
  });

  await manualStep("verify event stream", async () => {
    const response = await fetch(`${api}/events?session_id=${encodeURIComponent(sessionId)}`);
    const body = await response.text();
    assertIncludes(body, "session.created", "events session");
    assertIncludes(body, "artifact.added", "events artifact");
    assertIncludes(body, "decision.recorded", "events decision");
    assertIncludes(body, "context.packed", "events pack");
    assertIncludes(body, "adapter.registered", "events adapter");
  });
}

async function checked(
  name: string,
  command: string,
  args: string[],
  options: RunOptions = {},
): Promise<StepResult & { stdout: string; stderr: string }> {
  const result = await runCommand(name, command, args, options);
  steps.push(result);
  if (result.status === "fail" && !options.allowFailure) {
    throw new Error(`${name} failed`);
  }
  return result as StepResult & { stdout: string; stderr: string };
}

async function runCommand(
  name: string,
  command: string,
  args: string[],
  options: RunOptions,
): Promise<StepResult> {
  const started = performance.now();
  const display = commandDisplay(command, args);
  try {
    const child = Bun.spawn({
      cmd: [command, ...args],
      cwd: options.cwd ?? root,
      env: { ...process.env, ...options.env },
      stdout: "pipe",
      stderr: "pipe",
    });
    const [stdout, stderr, exitCode] = await Promise.all([
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
      child.exited,
    ]);
    const durationMs = Math.round(performance.now() - started);
    const base = {
      name,
      durationMs,
      command: display,
      exitCode,
      stdout: redactExcerpt(stdout),
      stderr: redactExcerpt(stderr),
    };
    if (exitCode === 0) {
      return { ...base, status: "pass" };
    }
    return {
      ...base,
      status: "fail",
      ...summarizeFailure({ command: display, exitCode, stdout, stderr }),
    };
  } catch (error) {
    const durationMs = Math.round(performance.now() - started);
    const message = error instanceof Error ? error.message : String(error);
    return {
      name,
      status: "fail",
      durationMs,
      command: display,
      exitCode: 127,
      stderr: redactExcerpt(message),
      ...summarizeFailure({ command: display, exitCode: 127, stderr: message }),
    };
  }
}

async function manualStep(name: string, fn: () => Promise<void>) {
  const started = performance.now();
  try {
    await fn();
    steps.push({
      name,
      status: "pass",
      durationMs: Math.round(performance.now() - started),
    });
  } catch (error) {
    const result = failedStepFromError(name, error, Math.round(performance.now() - started));
    steps.push(result);
    throw error;
  }
}

async function waitForHealth(api: string, timeoutMs: number) {
  const deadline = Date.now() + timeoutMs;
  let lastError = "";
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`${api}/healthz`);
      if (response.ok) {
        return;
      }
      lastError = `${response.status} ${await response.text()}`;
    } catch (error) {
      lastError = error instanceof Error ? error.message : String(error);
    }
    await Bun.sleep(100);
  }
  throw new Error(`daemon health check timed out: ${lastError}`);
}

async function pickPort(): Promise<number> {
  const net = await import("node:net");
  return new Promise((resolvePort, reject) => {
    const server = net.createServer();
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      server.close(() => {
        if (address && typeof address === "object") {
          resolvePort(address.port);
        } else {
          reject(new Error("failed to allocate port"));
        }
      });
    });
    server.on("error", reject);
  });
}

function executableName(name: string): string {
  return process.platform === "win32" ? `${name}.exe` : name;
}

await main();
