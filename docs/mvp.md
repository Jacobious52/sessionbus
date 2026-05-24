# MVP

The MVP proves one workflow:

> Never re-explain the same engineering task to multiple AI tools again.

## In scope

- Local Rust daemon with SQLite persistence.
- `aictx` CLI for start, doctor, active-session selection, add-file, note,
  coordination messages, decision, command capture, git context capture, pack,
  export, import, resume, and close.
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
aictx doctor
aictx start --repo "Fix flaky deploy"
aictx note "Issue only happens in staging"
aictx message "Please review the staging deploy hypothesis" --to reviewer --topic deploy --requires-response
aictx add-file service.yaml
aictx decision "Start with staging config"
aictx capture -- cargo test -p deploy
aictx workspace
aictx dogfood --for cursor --note "Ready to continue in Cursor"
aictx add-diff
aictx add-commit HEAD
aictx watch --once --workspace .
aictx show
aictx pack --for chatgpt
aictx resume --target cursor
aictx export --format json --for acp > sessionbus-pack.json
aictx import sessionbus-pack.json
```

The CLI prints generated ids for created records and prints context packs to
stdout for inspection before pasting or importing into another tool.

## Useful CLI loops

```bash
aictx current
aictx list --active
aictx sessions --active
aictx use ses_...
aictx switch ses_...
aictx close
```

`aictx run -- <cmd>` executes a local command, streams its stdout/stderr back to
the terminal, and stores the output as a terminal or test-result artifact. Git
commands capture the current workspace status, dirty diff, or selected commit
patch as explicit artifacts.

`aictx capture -- <cmd>` is the automation-friendly alias for command capture.
`aictx shell-init zsh|bash|fish` prints shell helpers, including
`aictx-capture`, and `aictx watch --once --workspace .` captures a workspace
state artifact. Without `--once`, `watch` polls and records a new artifact when
the workspace status changes.

`aictx dogfood --for chatgpt|claude|cursor|acp|generic` prepares a handoff for
the next AI tool by recording the current workspace state, capturing the dirty
git diff when one exists, adding an optional `--note`, and printing a redacted
deterministic context pack to stdout. Capture bookkeeping is written to stderr
so stdout stays pasteable.

`aictx doctor` checks daemon reachability, workspace facts, and current-session
resolution. `aictx mcp` starts or reuses the local daemon, then runs a stdio MCP
server with tools for current session lookup, dogfood handoffs, pack rendering,
artifacts, events, workspace facts, notes, decisions, and coordination messages.
It also exposes `sessionbus://current/pack?profile=generic` as a readable
resource.

## Coordination boundary

Sessionbus can support cross-agent coordination through durable events, notes,
decisions, and artifacts in a shared local session. The MVP intentionally does
not provide direct agent-to-agent chat, routing, or task delegation. Agents
coordinate by writing inspectable state that humans and other tools can replay.

## Security defaults

- Loopback daemon by default.
- Per-user data directory.
- No network service beyond the local API.
- Explicit file snapshots only.
- Redaction runs before pack rendering.
- Adapters are sidecar processes with declared capabilities.
