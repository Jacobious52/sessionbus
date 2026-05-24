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
