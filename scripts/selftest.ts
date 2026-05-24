#!/usr/bin/env bun
import { existsSync } from "node:fs";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { homedir, tmpdir } from "node:os";
import { delimiter, dirname, join, resolve } from "node:path";
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

process.env.PATH = [join(homedir(), ".cargo", "bin"), process.env.PATH ?? ""].join(delimiter);

type RunOptions = {
  cwd?: string;
  env?: Record<string, string>;
  allowFailure?: boolean;
};

type CommandResult = StepResult & {
  stdout: string;
  stderr: string;
  reportStep: StepResult;
};

async function main() {
  try {
    await checked("check Bun runtime", "bun", ["--version"]);
    await checked("check Bun package manager", "bun", ["pm", "version"]);
    await checked("build TypeScript workspace", "bun", ["run", "build"]);
    await checked("test TypeScript workspace", "bun", ["run", "test"]);
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
  await checked("initialize git workspace", "git", ["init"], { cwd: workspace });
  await checked("configure git user name", "git", ["config", "user.name", "Sessionbus Selftest"], {
    cwd: workspace,
  });
  await checked("configure git user email", "git", [
    "config",
    "user.email",
    "selftest@sessionbus.local",
  ], { cwd: workspace });
  await checked("stage initial git file", "git", ["add", "service.yaml"], { cwd: workspace });
  await checked("commit initial git file", "git", ["commit", "-m", "initial service"], {
    cwd: workspace,
  });
  await writeFile(servicePath, "name: api\nTOKEN=super-secret\nreplicas: 2\n", "utf8");

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

  const doctor = await checked("run doctor through CLI", aictx, [
    "--api",
    api,
    "doctor",
  ], { env: baseEnv, cwd: workspace });
  await manualStep("verify doctor output", async () => {
    assertIncludes(doctor.stdout, "daemon\tok", "doctor daemon");
    assertIncludes(doctor.stdout, "workspace", "doctor workspace");
  });

  const start = await checked("create session through CLI", aictx, [
    "--api",
    api,
    "start",
    "--repo",
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

  const policyInit = await checked("initialize redaction policy", aictx, [
    "--api",
    api,
    "policy",
    "init",
  ], { env: baseEnv, cwd: workspace });
  await manualStep("verify policy init", async () => {
    assertIncludes(policyInit.stdout, ".sessionbus/policy.toml", "policy path");
  });
  await writeFile(join(workspace, ".sessionbus", "policy.toml"), 'redact_keys = ["CLIENT_ID"]\n', "utf8");

  const redactTest = await checked("test redaction policy", aictx, [
    "--api",
    api,
    "redact",
    "test",
    "CLIENT_ID=company-internal",
  ], { env: baseEnv, cwd: workspace });
  await manualStep("verify custom redaction policy", async () => {
    assertIncludes(redactTest.stdout, "CLIENT_ID=[REDACTED]", "custom redaction");
    assertExcludes(redactTest.stdout, "company-internal", "custom secret leakage");
  });

  await checked("add file snapshot through CLI", aictx, [
    "--api",
    api,
    "add-file",
    servicePath,
  ], { env: baseEnv, cwd: workspace });

  const customSecretPath = join(workspace, "client.env");
  await writeFile(customSecretPath, "CLIENT_ID=company-internal\n", "utf8");
  await checked("add custom policy file snapshot", aictx, [
    "--api",
    api,
    "add-file",
    customSecretPath,
  ], { env: baseEnv, cwd: workspace });

  await checked("add decision through CLI", aictx, [
    "--api",
    api,
    "decision",
    "Start from staging config",
  ], { env: baseEnv, cwd: workspace });

  await checked("add coordination message through CLI", aictx, [
    "--api",
    api,
    "message",
    "add",
    "Please review the staging deploy hypothesis",
    "--to",
    "review-agent",
    "--topic",
    "staging deploy",
    "--requires-response",
  ], { env: baseEnv, cwd: workspace });

  const messageListOpen = await checked("list open coordination messages", aictx, [
    "--api",
    api,
    "message",
    "list",
  ], { env: baseEnv, cwd: workspace });
  let messageId = "";
  await manualStep("verify open coordination message", async () => {
    assertIncludes(messageListOpen.stdout, "review-agent", "message recipient");
    assertIncludes(messageListOpen.stdout, "open", "message open status");
    messageId = messageListOpen.stdout.trim().split(/\s+/)[0] ?? "";
    if (!messageId.startsWith("art_")) {
      throw new Error(`expected message artifact id, got: ${messageListOpen.stdout}`);
    }
  });

  await checked("ack coordination message", aictx, [
    "--api",
    api,
    "message",
    "ack",
    messageId,
  ], { env: baseEnv, cwd: workspace });

  await checked("resolve coordination message", aictx, [
    "--api",
    api,
    "message",
    "resolve",
    messageId,
  ], { env: baseEnv, cwd: workspace });

  const messageListResolved = await checked("list resolved coordination messages", aictx, [
    "--api",
    api,
    "message",
    "list",
    "--status",
    "resolved",
  ], { env: baseEnv, cwd: workspace });
  await manualStep("verify resolved coordination message", async () => {
    assertIncludes(messageListResolved.stdout, messageId, "resolved message id");
    assertIncludes(messageListResolved.stdout, "resolved", "resolved message status");
  });

  const sessionDoctor = await checked("run session doctor", aictx, [
    "--api",
    api,
    "session",
    "doctor",
  ], { env: baseEnv, cwd: workspace });
  await manualStep("verify session doctor", async () => {
    assertIncludes(sessionDoctor.stdout, sessionId, "session doctor id");
    assertIncludes(sessionDoctor.stdout, "Selftest continuity", "session doctor title");
  });

  const sessionSuggest = await checked("suggest session", aictx, [
    "--api",
    api,
    "session",
    "suggest",
  ], { env: baseEnv, cwd: workspace });
  await manualStep("verify session suggest", async () => {
    assertIncludes(sessionSuggest.stdout, sessionId, "session suggest id");
  });

  await checked("unbind workspace session", aictx, [
    "--api",
    api,
    "session",
    "unbind",
  ], { env: baseEnv, cwd: workspace });

  await checked("bind workspace session", aictx, [
    "--api",
    api,
    "session",
    "bind",
    "--repo",
    "--session",
    sessionId,
  ], { env: baseEnv, cwd: workspace });

  const current = await checked("show current session id", aictx, [
    "--api",
    api,
    "current",
  ], { env: baseEnv, cwd: workspace });
  await manualStep("verify current session output", async () => {
    assertIncludes(current.stdout, sessionId, "current session");
  });

  await checked("capture command through CLI", aictx, [
    "--api",
    api,
    "run",
    "--",
    "bun",
    "--version",
  ], { env: baseEnv, cwd: workspace });

  await checked("capture command through automation alias", aictx, [
    "--api",
    api,
    "capture",
    "--",
    "bun",
    "--version",
  ], { env: baseEnv, cwd: workspace });

  const shellInit = await checked("print zsh shell integration", aictx, [
    "--api",
    api,
    "shell-init",
    "zsh",
  ], { env: baseEnv, cwd: workspace });
  await manualStep("verify shell integration output", async () => {
    assertIncludes(shellInit.stdout, "aictx-capture", "shell capture function");
    assertIncludes(shellInit.stdout, "aictx capture --", "shell capture command");
  });

  const workspaceInfo = await checked("show git workspace through CLI", aictx, [
    "--api",
    api,
    "workspace",
  ], { env: baseEnv, cwd: workspace });
  await manualStep("verify git workspace output", async () => {
    assertIncludes(workspaceInfo.stdout, "branch", "workspace branch");
    assertIncludes(workspaceInfo.stdout, "service.yaml", "workspace dirty file");
  });

  await checked("capture git diff through CLI", aictx, [
    "--api",
    api,
    "add-diff",
  ], { env: baseEnv, cwd: workspace });

  await checked("capture git commit through CLI", aictx, [
    "--api",
    api,
    "add-commit",
    "HEAD",
  ], { env: baseEnv, cwd: workspace });

  await checked("capture workspace watch snapshot", aictx, [
    "--api",
    api,
    "watch",
    "--once",
    "--workspace",
    workspace,
  ], { env: baseEnv, cwd: workspace });

  const childWorkspace = join(workspace, "src");
  await mkdir(childWorkspace, { recursive: true });
  const currentFromChild = await checked("resolve workspace session from child directory", aictx, [
    "--api",
    api,
    "current",
  ], { env: baseEnv, cwd: childWorkspace });
  await manualStep("verify child directory uses workspace session", async () => {
    assertIncludes(currentFromChild.stdout, sessionId, "workspace current session");
  });

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
    assertIncludes(show.stdout, "bun --version", "show command capture");
    assertIncludes(show.stdout, "git diff", "show git diff");
    assertIncludes(show.stdout, "git commit HEAD", "show git commit");
    assertIncludes(show.stdout, "workspace watch", "show workspace watch");
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
    assertIncludes(pack.stdout, "CLIENT_ID=[REDACTED]", "pack custom redaction");
    assertExcludes(pack.stdout, "super-secret", "pack secret leakage");
    assertExcludes(pack.stdout, "company-internal", "pack custom secret leakage");
  });

  const packPreview = await checked("preview pack through CLI", aictx, [
    "--api",
    api,
    "pack",
    "--preview",
  ], { env: baseEnv, cwd: workspace });
  await manualStep("verify pack preview", async () => {
    assertIncludes(packPreview.stdout, "Preview", "pack preview heading");
    assertIncludes(packPreview.stdout, "CLIENT_ID=[REDACTED]", "pack preview custom redaction");
  });

  const installCodex = await checked("print codex install helper", aictx, [
    "--api",
    api,
    "install",
    "codex",
  ], { env: baseEnv, cwd: workspace });
  await manualStep("verify codex install helper", async () => {
    assertIncludes(installCodex.stdout, "[mcp_servers.sessionbus]", "codex mcp config");
    assertIncludes(installCodex.stdout, "mcp --ensure-daemon", "codex ensure daemon");
  });

  const installShell = await checked("print shell install helper", aictx, [
    "--api",
    api,
    "install",
    "shell",
  ], { env: baseEnv, cwd: workspace });
  await manualStep("verify shell install helper", async () => {
    assertIncludes(installShell.stdout, "aictx shell-init", "shell install command");
  });

  const codexConfigPath = join(tempRoot, "codex-config.toml");
  await checked("write codex install config", aictx, [
    "--api",
    api,
    "install",
    "codex",
    "--write",
    "--config",
    codexConfigPath,
  ], { env: baseEnv, cwd: workspace });
  await checked("rewrite codex install config idempotently", aictx, [
    "--api",
    api,
    "install",
    "codex",
    "--write",
    "--config",
    codexConfigPath,
  ], { env: baseEnv, cwd: workspace });
  await manualStep("verify written codex install config", async () => {
    const config = await readFile(codexConfigPath, "utf8");
    assertIncludes(config, "[mcp_servers.sessionbus]", "written codex mcp config");
    assertIncludes(config, "mcp\", \"--ensure-daemon", "written codex ensure daemon");
    const occurrences = config.match(/\[mcp_servers\.sessionbus\]/g)?.length ?? 0;
    if (occurrences !== 1) {
      throw new Error(`expected one sessionbus mcp block, got ${occurrences}: ${config}`);
    }
  });

  const shellRcPath = join(tempRoot, "zshrc");
  await checked("write shell install config", aictx, [
    "--api",
    api,
    "install",
    "shell",
    "--write",
    "--shell",
    "zsh",
    "--rc",
    shellRcPath,
  ], { env: baseEnv, cwd: workspace });
  await checked("rewrite shell install config idempotently", aictx, [
    "--api",
    api,
    "install",
    "shell",
    "--write",
    "--shell",
    "zsh",
    "--rc",
    shellRcPath,
  ], { env: baseEnv, cwd: workspace });
  await manualStep("verify written shell install config", async () => {
    const rc = await readFile(shellRcPath, "utf8");
    assertIncludes(rc, "aictx shell-init zsh", "written shell init");
    const starts = rc.match(/sessionbus start/g)?.length ?? 0;
    if (starts !== 1) {
      throw new Error(`expected one shell install block, got ${starts}: ${rc}`);
    }
  });

  const dashboard = await checked("open dashboard through CLI", aictx, [
    "--api",
    api,
    "dashboard",
    "--print-url",
  ], { env: baseEnv, cwd: workspace });
  await manualStep("verify dashboard CLI output and daemon page", async () => {
    assertIncludes(dashboard.stdout, `${api}/dashboard`, "dashboard url");
    const page = await fetch(`${api}/dashboard`).then((response) => response.text());
    assertIncludes(page, "Sessionbus", "dashboard brand");
    assertIncludes(page, "Never re-explain", "dashboard problem framing");
    assertIncludes(page, "Start Session", "dashboard start control");
    assertIncludes(page, "Add Note", "dashboard note control");
    assertIncludes(page, "Render Pack", "dashboard pack control");
    const data = await fetch(`${api}/api/dashboard`).then((response) => response.json());
    if (!Array.isArray(data.sessions) || data.sessions.length === 0) {
      throw new Error(`expected dashboard sessions, got ${JSON.stringify(data)}`);
    }
    const dashboardSession = await fetch(`${api}/sessions`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        title: "Dashboard-created session",
        summary: "Created through dashboard-compatible API control flow.",
      }),
    }).then((response) => response.json());
    if (!dashboardSession.id?.startsWith("ses_")) {
      throw new Error(`expected dashboard session id, got ${JSON.stringify(dashboardSession)}`);
    }
    await fetch(`${api}/sessions/${dashboardSession.id}/artifacts`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        kind: "note",
        title: "dashboard note",
        body: "Dashboard control note",
        metadata: { source: "dashboard-selftest" },
        snapshot: true,
      }),
    });
    const dashboardPack = await fetch(`${api}/sessions/${dashboardSession.id}/pack`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ profile: "generic" }),
    }).then((response) => response.json());
    assertIncludes(dashboardPack.markdown, "Dashboard-created session", "dashboard pack title");
    assertIncludes(dashboardPack.markdown, "Dashboard control note", "dashboard pack note");
  });

  const dashboardOpen = await checked("open dashboard with override command", aictx, [
    "--api",
    api,
    "dashboard",
  ], { env: { ...baseEnv, SESSIONBUS_OPEN_COMMAND: "/bin/echo" }, cwd: workspace });
  await manualStep("verify dashboard opener output", async () => {
    assertIncludes(dashboardOpen.stdout, `${api}/dashboard`, "dashboard opener url");
  });

  const cursorPack = await checked("pack cursor profile through CLI", aictx, [
    "--api",
    api,
    "pack",
    "--for",
    "cursor",
  ], { env: baseEnv, cwd: workspace });
  await manualStep("verify cursor pack profile", async () => {
    assertIncludes(cursorPack.stdout, "# Cursor Handoff", "cursor pack heading");
    assertIncludes(cursorPack.stdout, "Workspace-first context", "cursor pack guidance");
  });

  const acpPackJson = await checked("export acp json pack through CLI", aictx, [
    "--api",
    api,
    "export",
    "--format",
    "json",
    "--for",
    "acp",
  ], { env: baseEnv, cwd: workspace });
  const importPath = join(tempRoot, "sessionbus-pack.json");
  await writeFile(importPath, acpPackJson.stdout, "utf8");
  await manualStep("verify acp json pack profile", async () => {
    const parsed = JSON.parse(acpPackJson.stdout);
    if (parsed.profile !== "acp") {
      throw new Error(`expected acp profile, got ${parsed.profile}`);
    }
  });

  const imported = await checked("import pack through CLI", aictx, [
    "--api",
    api,
    "import",
    importPath,
  ], { env: baseEnv, cwd: workspace });
  const importedSessionId = imported.stdout.trim().split(/\s+/).at(-1);
  if (!importedSessionId?.startsWith("ses_")) {
    throw new Error(`expected imported session id, got: ${imported.stdout}`);
  }

  await checked("switch back to original session", aictx, [
    "--api",
    api,
    "use",
    sessionId,
  ], { env: baseEnv, cwd: workspace });

  const activeList = await checked("list active sessions through CLI", aictx, [
    "--api",
    api,
    "list",
    "--active",
  ], { env: baseEnv, cwd: workspace });
  await manualStep("verify active session list", async () => {
    assertIncludes(activeList.stdout, sessionId, "active list original session");
  });

  const sessions = await checked("list sessions through sessions alias", aictx, [
    "--api",
    api,
    "sessions",
    "--active",
  ], { env: baseEnv, cwd: workspace });
  await manualStep("verify sessions alias", async () => {
    assertIncludes(sessions.stdout, sessionId, "sessions alias original session");
  });

  await checked("close current session through CLI", aictx, [
    "--api",
    api,
    "close",
  ], { env: baseEnv, cwd: workspace });

  const closedList = await checked("list active sessions after close", aictx, [
    "--api",
    api,
    "list",
    "--active",
  ], { env: baseEnv, cwd: workspace });
  await manualStep("verify closed session removed from active list", async () => {
    assertExcludes(closedList.stdout, sessionId, "closed active list");
  });

  await checked("switch to imported session", aictx, [
    "--api",
    api,
    "switch",
    importedSessionId,
  ], { env: baseEnv, cwd: workspace });

  await manualStep("exercise MCP stdio server", async () => {
    const responses = await runMcpExchange(aictx, baseEnv, workspace, api, false);
    const initialize = responses.find((response) => response.id === 1);
    const tools = responses.find((response) => response.id === 2);
    const packTool = responses.find((response) => response.id === 3);
    const resources = responses.find((response) => response.id === 4);
    const resource = responses.find((response) => response.id === 5);
    const workspaceTool = responses.find((response) => response.id === 6);
    const artifactsTool = responses.find((response) => response.id === 7);
    const eventsTool = responses.find((response) => response.id === 8);
    const messageTool = responses.find((response) => response.id === 9);
    if (!initialize?.result?.serverInfo?.name?.includes("sessionbus")) {
      throw new Error(`missing MCP initialize response: ${JSON.stringify(initialize)}`);
    }
    const toolNames = tools?.result?.tools?.map((tool: { name: string }) => tool.name) ?? [];
    if (
      !toolNames.includes("sessionbus_pack") ||
      !toolNames.includes("sessionbus_note") ||
      !toolNames.includes("sessionbus_events") ||
      !toolNames.includes("sessionbus_message")
    ) {
      throw new Error(`missing Sessionbus MCP tools: ${JSON.stringify(toolNames)}`);
    }
    assertIncludes(JSON.stringify(packTool), "Selftest continuity", "MCP pack tool");
    assertIncludes(JSON.stringify(resources), "sessionbus://current/pack", "MCP resources list");
    assertIncludes(JSON.stringify(resource), "Selftest continuity", "MCP resource read");
    assertIncludes(JSON.stringify(workspaceTool), "service.yaml", "MCP workspace tool");
    assertIncludes(JSON.stringify(artifactsTool), "service.yaml", "MCP artifacts tool");
    assertIncludes(JSON.stringify(eventsTool), "session.created", "MCP events tool");
    assertIncludes(JSON.stringify(messageTool), "coordination message", "MCP message tool");
  });

  await manualStep("exercise MCP ensure-daemon startup", async () => {
    const ensuredPort = await pickPort();
    const ensuredApi = `http://127.0.0.1:${ensuredPort}`;
    const responses = await runMcpExchange(aictx, baseEnv, workspace, ensuredApi, true);
    const initialize = responses.find((response) => response.id === 1);
    const packTool = responses.find((response) => response.id === 3);
    if (!initialize?.result?.serverInfo?.name?.includes("sessionbus")) {
      throw new Error(`missing ensured MCP initialize response: ${JSON.stringify(initialize)}`);
    }
    assertIncludes(JSON.stringify(packTool), "Selftest continuity", "ensured MCP pack tool");
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
): Promise<CommandResult> {
  const result = await runCommand(name, command, args, options);
  steps.push(result.reportStep);
  if (result.status === "fail" && !options.allowFailure) {
    throw new Error(`${name} failed`);
  }
  return result;
}

async function runCommand(
  name: string,
  command: string,
  args: string[],
  options: RunOptions,
): Promise<CommandResult> {
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
      stdout,
      stderr,
    };
    const reportBase = {
      name,
      durationMs,
      command: display,
      exitCode,
      stdout: redactExcerpt(stdout),
      stderr: redactExcerpt(stderr),
    };
    if (exitCode === 0) {
      return {
        ...base,
        status: "pass",
        reportStep: { ...reportBase, status: "pass" },
      };
    }
    const summary = summarizeFailure({ command: display, exitCode, stdout, stderr });
    return {
      ...base,
      status: "fail",
      ...summary,
      reportStep: {
        ...reportBase,
        status: "fail",
        ...summary,
      },
    };
  } catch (error) {
    const durationMs = Math.round(performance.now() - started);
    const message = error instanceof Error ? error.message : String(error);
    const summary = summarizeFailure({ command: display, exitCode: 127, stderr: message });
    return {
      name,
      status: "fail",
      durationMs,
      command: display,
      exitCode: 127,
      stdout: "",
      stderr: message,
      ...summary,
      reportStep: {
        name,
        status: "fail",
        durationMs,
        command: display,
        exitCode: 127,
        stdout: "",
        stderr: redactExcerpt(message),
        ...summary,
      },
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

async function runMcpExchange(
  aictx: string,
  env: Record<string, string>,
  cwd: string,
  api: string,
  ensureDaemon: boolean,
): Promise<any[]> {
  const cmd = [aictx, "--api", api, "mcp"];
  if (ensureDaemon) {
    cmd.push("--ensure-daemon");
  }
  const child = Bun.spawn({
    cmd,
    cwd,
    env,
    stdin: "pipe",
    stdout: "pipe",
    stderr: "pipe",
  });
  const requests = [
    {
      jsonrpc: "2.0",
      id: 1,
      method: "initialize",
      params: {
        protocolVersion: "2025-11-25",
        capabilities: {},
        clientInfo: { name: "sessionbus-selftest", version: "0.1.0" },
      },
    },
    { jsonrpc: "2.0", method: "notifications/initialized" },
    { jsonrpc: "2.0", id: 2, method: "tools/list" },
    {
      jsonrpc: "2.0",
      id: 3,
      method: "tools/call",
      params: { name: "sessionbus_pack", arguments: { profile: "generic" } },
    },
    { jsonrpc: "2.0", id: 4, method: "resources/list" },
    {
      jsonrpc: "2.0",
      id: 5,
      method: "resources/read",
      params: { uri: "sessionbus://current/pack?profile=generic" },
    },
    {
      jsonrpc: "2.0",
      id: 6,
      method: "tools/call",
      params: { name: "sessionbus_workspace", arguments: {} },
    },
    {
      jsonrpc: "2.0",
      id: 7,
      method: "tools/call",
      params: { name: "sessionbus_artifacts", arguments: {} },
    },
    {
      jsonrpc: "2.0",
      id: 8,
      method: "tools/call",
      params: { name: "sessionbus_events", arguments: {} },
    },
    {
      jsonrpc: "2.0",
      id: 9,
      method: "tools/call",
      params: {
        name: "sessionbus_message",
        arguments: {
          to_agent: "codex",
          topic: "coordination",
          text: "coordination message from MCP",
          requires_response: true,
        },
      },
    },
  ];
  for (const request of requests) {
    child.stdin.write(`${JSON.stringify(request)}\n`);
  }
  child.stdin.end();
  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
    child.exited,
  ]);
  if (exitCode !== 0) {
    throw new Error(`MCP server exited ${exitCode}: ${stderr}`);
  }
  return stdout
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line));
}

function executableName(name: string): string {
  return process.platform === "win32" ? `${name}.exe` : name;
}

await main();
