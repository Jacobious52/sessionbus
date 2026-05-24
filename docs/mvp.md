# MVP

The MVP proves one workflow:

> Never re-explain the same engineering task to multiple AI tools again.

## In scope

- Local Rust daemon with SQLite persistence.
- `aictx` CLI for start, add-file, note, decision, pack, export, and resume.
- Deterministic context packs for ChatGPT, Claude, Cursor, ACP, and generic
  Markdown/JSON handoff.
- Sidecar adapter capability registration.
- TypeScript SDK for adapter authors.
- Example terminal and filesystem adapters.
- ACP bridge skeleton as a first-class adapter.

## Out of scope

- Cloud sync.
- Autonomous agents.
- Model/provider APIs.
- Chat transcript synchronization.
- In-process plugin hosting.
- MCP, IDE, and browser-extension adapters beyond documentation placeholders.

## Demo

```bash
aictx daemon
aictx status
aictx start "Fix flaky deploy"
aictx note "Issue only happens in staging"
aictx add-file service.yaml
aictx decision "Start with staging config"
aictx show
aictx pack --for chatgpt
aictx resume --target cursor
```

The CLI prints generated ids for created records and prints context packs to
stdout for inspection before pasting or importing into another tool.

## Security defaults

- Loopback daemon by default.
- Per-user data directory.
- No network service beyond the local API.
- Explicit file snapshots only.
- Redaction runs before pack rendering.
- Adapters are sidecar processes with declared capabilities.
