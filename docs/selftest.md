# Selftest Harness

The selftest harness is designed for humans and AI agents that need an
end-to-end feedback loop.

## Commands

```bash
npm run selftest:unit
npm run selftest
```

`npm run selftest` uses Bun as the TypeScript runtime:

```text
node scripts/run-selftest.mjs
```

The Node wrapper resolves Bun from `PATH` or the common `~/.bun/bin/bun` install
path, prepends `~/.bun/bin` and `~/.cargo/bin` for the harness subprocesses, and
produces a clear report when Bun is missing. The actual E2E harness runs under
Bun.

## What it checks

- Bun runtime availability.
- npm availability.
- TypeScript workspace build and tests.
- Rust toolchain availability.
- Rust format, tests, and workspace build.
- Real `aictx` daemon startup on a random localhost port.
- Real CLI flow: start, note, add-file, decision, pack.
- Pack redaction and expected context content.
- ACP bridge registration.
- NDJSON event stream contents.

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
