# Contributing

Thanks for helping make Sessionbus useful.

## Development Loop

Install dependencies:

```bash
bun install
cargo build --workspace
```

Run the full verification harness:

```bash
bun run selftest
```

The selftest is the project gate. It builds TypeScript packages, runs Rust
format/tests/build, starts a real daemon, drives the CLI, exercises MCP, checks
redaction, and verifies the dashboard.

## Design Boundaries

Sessionbus is:

- local-first
- CLI-first
- MCP-aware
- adapter-oriented
- a durable engineering task/session state layer

Sessionbus is not:

- an autonomous agent runtime
- a chat UI
- a model wrapper
- a cloud orchestration service

Prefer small, inspectable primitives over hidden automation. If a feature writes
state, it should be visible as a session, artifact, decision, event, or pack.

## Pull Requests

- Keep changes scoped.
- Add or update selftest coverage for user-visible behavior.
- Keep secrets out of fixtures and examples.
- Run `bun run selftest` before opening a PR.
