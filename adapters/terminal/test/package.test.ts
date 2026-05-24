import { expect, test } from "bun:test";
import pkg from "../package.json";
import { observeArtifact, parseObserveArgs, shellInit, shellQuote } from "../src/index";

test("terminal adapter uses Bun for tests and exposes a build script", () => {
  expect(pkg.scripts.test).toContain("bun test");
  expect(pkg.scripts.build).toContain("tsc");
});

test("terminal adapter parses observed command metadata", () => {
  expect(
    parseObserveArgs([
      "--session",
      "ses_123",
      "--shell",
      "zsh",
      "--exit-code",
      "7",
      "--duration-ms",
      "1234",
      "--",
      "cargo",
      "test",
      "--workspace",
    ]),
  ).toEqual({
    sessionId: "ses_123",
    shell: "zsh",
    exitCode: 7,
    durationMs: 1234,
    commandLine: "cargo test --workspace",
  });
});

test("terminal adapter builds inspectable command artifacts", () => {
  const artifact = observeArtifact({
    sessionId: "ses_123",
    commandLine: "bun run selftest",
    shell: "zsh",
    exitCode: 0,
    durationMs: 42,
  });
  expect(artifact.kind).toBe("tool_invocation");
  expect(artifact.body).toContain("$ bun run selftest");
  expect(artifact.body).toContain("exit_code\t0");
  expect(artifact.metadata).toMatchObject({
    adapter: "terminal",
    source: "terminal-adapter",
    command_line: "bun run selftest",
  });
});

test("terminal adapter emits shell hooks that call the adapter", () => {
  const hook = shellInit("zsh", "ses_123", "bun adapters/terminal/src/index.ts");
  expect(hook).toContain("SESSIONBUS_SESSION=\"ses_123\"");
  expect(hook).toContain("observe --session");
  expect(hook).toContain("add-zsh-hook");
});

test("terminal adapter quotes command paths for shell hooks", () => {
  expect(shellQuote("/tmp/sessionbus adapter/index.ts")).toBe("'/tmp/sessionbus adapter/index.ts'");
  expect(shellQuote("bun")).toBe("bun");
});
