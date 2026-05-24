# Local Protocol

The v0 protocol is HTTP JSON for commands and NDJSON for event export.

Default daemon address:

```text
http://127.0.0.1:8765
```

## Endpoints

```text
POST /sessions
GET  /sessions
GET  /sessions/:id
POST /sessions/:id/status
POST /sessions/:id/artifacts
POST /sessions/:id/decisions
POST /sessions/:id/pack
GET  /events?session_id=:id
POST /adapters/register
```

## Create a session

```json
{
  "title": "Fix flaky deploy",
  "summary": "Deploy fails intermittently in staging.",
  "workspace": {
    "root": "/repo",
    "git_branch": "main",
    "head": "abc123"
  }
}
```

## Update session status

```json
{
  "status": "done"
}
```

Allowed statuses are `active`, `blocked`, `done`, and `archived`.

## Add an artifact

```json
{
  "kind": "terminal_output",
  "title": "failing deploy output",
  "body": "TOKEN=secret\nstaging failed",
  "metadata": { "exit_code": 1 },
  "snapshot": true
}
```

When `snapshot` is true and a body is present, the store records a
content-addressed reference. Packs run redaction before rendering body content.

## Register an adapter

```json
{
  "adapter_id": "sessionbus.acp-bridge",
  "protocol": "acp",
  "version": "0.1.0",
  "capabilities": [
    "import_context",
    "export_context",
    "stream_updates",
    "session_resume",
    "session_observe"
  ],
  "metadata": {
    "role": "bridge"
  }
}
```

## Event stream

`GET /events` returns newline-delimited JSON:

```jsonl
{"id":"evt_...","type":"session.created","source":"sessionbus-store","payload":{},"created_at":"..."}
{"id":"evt_...","type":"artifact.added","source":"sessionbus-store","payload":{},"created_at":"..."}
```
