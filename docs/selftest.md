# Selftest Harness

The selftest harness is designed for humans and AI agents that need an
end-to-end feedback loop.

## Commands

```bash
bun install
bun run selftest:unit
bun run selftest
```

`bun run selftest` uses Bun as the TypeScript runtime and package runner:

```text
bun run scripts/selftest.ts
```

The E2E harness runs under Bun and prepends the common `~/.cargo/bin` location
for Rust subprocesses.

## What it checks

- Bun runtime availability.
- Bun package manager availability.
- TypeScript workspace build and tests.
- Rust toolchain availability.
- Rust format, tests, and workspace build.
- Real `aictx` daemon startup on a random localhost port.
- Real CLI flow: status, doctor, start, current, note, coordination message,
  add-file, decision, command capture, git workspace inspection, git diff
  capture, git commit capture, automation capture alias, shell helper
  generation, workspace watch snapshot, repo-local active session resolution,
  show, pack, export, import, switch/use, sessions/list active, and close.
- Pack redaction, profile-specific context content, and JSON importability.
- MCP stdio initialize, tools/list, tools/call, resources/list, resources/read,
  richer Sessionbus tools, and `--ensure-daemon` startup flow.
- ACP bridge registration.
- NDJSON event stream contents.
- Report output redaction without truncating the raw command output used by
  assertions.

## Report

The harness writes `selftest-report.json` at the repo root:

```json
{
  "status": "fail",
  "summary": "Selftest failed at \"check Rust toolchain\".",
  "failedStep": {
    "name": "check Rust toolchain",
    "command": "cargo --version",
    "exitCode": 127,
    "likelyCause": "Rust toolchain is not installed or cargo is not on PATH.",
    "suggestedNextAction": "Install Rust with rustup, then rerun the selftest."
  }
}
```

This shape is intentionally stable so an AI agent can inspect the failed step,
make one fix, and rerun the harness.
