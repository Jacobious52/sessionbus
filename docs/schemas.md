# Schemas

The canonical schema source is `sessionbus-core`.

The public contracts are:

- `Session`
- `Artifact`
- `Decision`
- `BusEvent`
- `CapabilityDescriptor`
- `ContextPack`

Rust callers should depend on `sessionbus-core`. TypeScript adapter authors can
use `@sessionbus/adapter-sdk`, which mirrors the v0 wire types.

## Artifact kinds

```text
file
git_diff
terminal_output
stack_trace
url
chat_snippet
tool_invocation
test_result
note
ticket
```

## Adapter capabilities

```text
import_context
export_context
stream_updates
read_workspace
write_artifact
tool_calls
session_resume
session_observe
```

## Pack profiles

```text
chatgpt
claude
cursor
acp
generic
```
