# Security Policy

Sessionbus is local-first infrastructure. The default posture is to keep task
state on the developer's machine.

## Reporting

Please report security issues privately by opening a GitHub security advisory or
contacting the repository owner. Do not file public issues for suspected
vulnerabilities.

## Defaults

- The daemon binds to loopback by default.
- v0 has no cloud sync.
- File contents are captured only through explicit commands or adapters.
- Context packs run redaction before output.
- Adapters run out of process and declare capabilities.

## Sensitive Data

Use repo-local policy files for additional redaction:

```bash
aictx policy init
```

Then edit `.sessionbus/policy.toml`:

```toml
redact_keys = ["CLIENT_ID", "INTERNAL_TOKEN"]
```

Check a candidate value:

```bash
aictx redact test "CLIENT_ID=company-internal"
```
