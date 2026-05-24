# Sessionbus

Sessionbus is a local-first continuity layer for engineering work that moves
across AI tools. The CLI is `aictx`.

The project stores durable task state, artifacts, decisions, and deterministic
context packs. It is not an agent framework, chat UI, model wrapper, or cloud
orchestration platform.

## Five-minute demo

```bash
aictx daemon
aictx status
aictx start "Fix flaky deploy"
aictx note "Issue only happens in staging"
aictx add-file service.yaml
aictx show
aictx pack --for chatgpt
```

The MVP is CLI-first and local-only. Tool integrations are sidecar adapters
that register capabilities with the local daemon and degrade to import/export
packs when richer integration is unavailable.

## Workspace

- `crates/sessionbus-core`: domain types, JSON schemas, deterministic packer.
- `crates/sessionbus-store`: SQLite event log and projections.
- `crates/sessionbus-daemon`: local HTTP and NDJSON API.
- `crates/aictx-cli`: CLI entrypoint.
- `crates/sessionbus-acp-bridge`: first-class ACP bridge skeleton.
- `packages/adapter-sdk-ts`: TypeScript adapter SDK.
- `adapters/*`: example adapters.

See `docs/protocol.md`, `docs/adapters.md`, and `docs/security.md` for the
initial daemon API, capability negotiation, and privacy boundaries.

## Selftest

The project includes a Bun-powered harness for autonomous AI feedback loops:

```bash
npm run selftest
```

The harness writes `selftest-report.json` with the failed step, command output,
likely cause, and suggested next action. See `docs/selftest.md`.
