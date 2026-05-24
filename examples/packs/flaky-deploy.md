# Fix flaky deploy

- Session: `ses_example`
- Status: `active`
- Target profile: `chatgpt`

## Intent

Deploy fails intermittently in staging.

## Workspace

- Root: `/repo`
- Git branch: `main`
- Head: `abc123`

## Decisions

- Start with staging config.

## Artifacts

### note: note

```text
Issue only happens in staging.
```

### terminal_output: failing deploy output

```text
TOKEN=[REDACTED]
staging failed
```

## Handoff

Continue from this engineering task state. Preserve decisions, use artifacts as
evidence, and ask for missing information rather than assuming it.
