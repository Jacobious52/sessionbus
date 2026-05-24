import assert from "node:assert/strict";
import test from "node:test";
import {
  assertIncludes,
  buildReport,
  commandDisplay,
  redactExcerpt,
  summarizeFailure,
} from "./selftest-lib.ts";

test("commandDisplay quotes arguments with whitespace", () => {
  assert.equal(
    commandDisplay("aictx", ["start", "Fix flaky deploy"]),
    'aictx start "Fix flaky deploy"',
  );
});

test("redactExcerpt removes secret assignment values", () => {
  assert.equal(redactExcerpt("TOKEN=super-secret\nok"), "TOKEN=[REDACTED]\nok");
});

test("assertIncludes reports missing expected content", () => {
  assert.throws(
    () => assertIncludes("hello", "TOKEN=[REDACTED]", "pack redaction"),
    /pack redaction/,
  );
});

test("buildReport records failed step with likely cause and action", () => {
  const report = buildReport([
    {
      name: "check bun",
      status: "pass",
      durationMs: 5,
      command: "bun --version",
    },
    {
      name: "cargo test",
      status: "fail",
      durationMs: 8,
      command: "cargo test",
      exitCode: 127,
      stderr: "command not found: cargo",
      likelyCause: "Rust toolchain is not installed or cargo is not on PATH.",
      suggestedNextAction: "Install Rust with rustup, then rerun the selftest.",
    },
  ]);

  assert.equal(report.status, "fail");
  assert.equal(report.failedStep?.name, "cargo test");
  assert.match(report.summary, /cargo test/);
});

test("summarizeFailure diagnoses missing Bun", () => {
  const summary = summarizeFailure({
    command: "bun --version",
    exitCode: 127,
    stderr: "command not found: bun",
  });

  assert.equal(summary.likelyCause, "Bun is not installed or not on PATH.");
  assert.match(summary.suggestedNextAction, /bun install/);
});

test("summarizeFailure diagnoses Bun executable-not-found cargo errors", () => {
  const summary = summarizeFailure({
    command: "cargo --version",
    stderr: 'Executable not found in $PATH: "cargo"',
    exitCode: 127,
  });

  assert.equal(summary.likelyCause, "Rust toolchain is not installed or cargo is not on PATH.");
  assert.match(summary.suggestedNextAction, /rustup/);
});

test("summarizeFailure diagnoses local bind restrictions", () => {
  const summary = summarizeFailure({
    command: "allocate daemon port",
    stderr: "Failed to listen at 127.0.0.1",
    exitCode: 1,
  });

  assert.equal(summary.likelyCause, "Localhost binding is blocked by the current sandbox or environment.");
  assert.match(summary.suggestedNextAction, /network permissions/);
});
