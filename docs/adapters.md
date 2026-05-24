# Adapter SDK

Adapters are sidecar processes. They are not loaded into the daemon.

An adapter starts by registering a `CapabilityDescriptor`:

```json
{
  "adapter_id": "sessionbus.terminal",
  "protocol": "native-http",
  "version": "0.1.0",
  "capabilities": ["write_artifact", "stream_updates"],
  "metadata": { "example": true }
}
```

The daemon records this descriptor as an event and stores the latest projection.
Callers should choose the best available behavior from the descriptor:

- `import_context`: target can ingest a context pack.
- `export_context`: target can produce portable context.
- `stream_updates`: target can observe event output.
- `read_workspace`: target can inspect workspace state.
- `write_artifact`: target can write artifacts to a session.
- `tool_calls`: target can describe tool calls as artifacts.
- `session_resume`: target can resume from a session pack.
- `session_observe`: target can report external session metadata.

## Graceful degradation

The expected fallback order is:

1. Native session resume or import when the target supports it.
2. Target-specific deterministic pack.
3. Generic Markdown/JSON export.
4. Manual paste/import by the engineer.

This keeps the system useful even when a vendor exposes no integration API.

## MCP surface

`aictx mcp` is a local stdio MCP server. It starts or reuses a loopback daemon
before serving MCP unless `--no-ensure-daemon` is passed. It is intentionally
thin: it exposes Sessionbus state to MCP clients without making the daemon an
agent runtime.

Tools:

- `sessionbus_current`: read the current durable engineering session.
- `sessionbus_pack`: render a deterministic context pack.
- `sessionbus_handoff`: render a target-specific handoff.
- `sessionbus_dogfood`: capture workspace handoff state, then render a pack.
- `sessionbus_artifacts`: list current-session artifacts.
- `sessionbus_events`: list durable event-log entries.
- `sessionbus_workspace`: inspect the local git workspace.
- `sessionbus_add_artifact`: add an explicit artifact.
- `sessionbus_note`: add an inspectable note artifact.
- `sessionbus_decision`: record a durable engineering decision.
- `sessionbus_message`: leave an inspectable coordination message.

Resources:

- `sessionbus://current/pack?profile=generic`: Markdown context pack for the
  current session.

This is the preferred first bridge for AI tools that already understand MCP.
Vendor-specific adapters can build on the same capability model later.

Coordination messages are stored as note artifacts with metadata such as
`to_agent`, `topic`, `requires_response`, and `status`. They are durable memos,
not direct agent-to-agent chat.

## Terminal adapter

The terminal adapter is the first concrete sidecar path for making Sessionbus
less manual. It is a Bun/TypeScript process that registers capabilities and can
write terminal artifacts without being loaded into the daemon:

```bash
bun adapters/terminal/src/index.ts register
bun adapters/terminal/src/index.ts observe --session ses_... --shell zsh --exit-code 0 -- cargo test
printf 'test output\n' | bun adapters/terminal/src/index.ts capture --session ses_... cargo test
```

For shell integration, emit hooks that call the adapter directly:

```bash
eval "$(bun adapters/terminal/src/index.ts shell-init zsh --session ses_...)"
```

This adapter currently records command lines, exit codes, duration, shell name,
and explicit terminal output. It intentionally keeps output capture explicit so
the default privacy boundary remains understandable.
