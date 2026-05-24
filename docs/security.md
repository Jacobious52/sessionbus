# Security And Privacy

Sessionbus starts local-only.

## Defaults

- Bind to `127.0.0.1` by default.
- Store data under the current user's local data directory.
- Do not sync to cloud services in v0.
- Capture file content only when the user explicitly adds a file or adapter.
- Redact common secret assignments before rendering context packs.
- Keep adapters out of process and require declared capabilities.

## Stored data

SQLite stores:

- sessions
- artifacts
- decisions
- append-only events
- adapter registrations
- generated pack records

Artifact bodies are intended for explicit snapshots and small text payloads. The
MVP stores these in SQLite with content hashes and references. A future content
store can move larger blobs out of projection tables without changing the public
artifact contract.

## Enterprise evolution

The local-first OSS core can grow enterprise features without becoming a SaaS:

- encrypted local stores
- repo-local policy files
- adapter allowlists
- audit export
- managed redaction policies
- SSO-aware bridge policy
- centralized configuration distribution

These should be additive controls around the local bus, not a replacement for
the local-first model.
