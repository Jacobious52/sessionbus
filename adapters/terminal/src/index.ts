import { createAdapterDescriptor, SessionbusClient, type CreateArtifactRequest } from "@sessionbus/adapter-sdk";

const adapterId = "sessionbus.terminal";
const client = new SessionbusClient();

export type ObserveOptions = {
  sessionId: string;
  commandLine: string;
  exitCode?: number;
  durationMs?: number;
  shell?: string;
};

async function main() {
  const [command, ...args] = process.argv.slice(2);

  if (command === "register") {
    await client.registerAdapter(
      createAdapterDescriptor({
        adapter_id: adapterId,
        protocol: "native-http",
        version: "0.1.0",
        capabilities: ["write_artifact", "stream_updates", "session_observe"],
        metadata: {
          runtime: "bun",
          install: "bun adapters/terminal/src/index.ts shell-init zsh --session <session-id>",
        },
      }),
    );
    console.log(adapterId);
    return;
  }

  if (command === "capture") {
    const { sessionId, rest } = requireSession(args, "capture");
    const body = await readStdin();
    const artifact = await client.addArtifact(sessionId, {
      kind: "terminal_output",
      title: rest.join(" ") || "terminal output",
      body,
      metadata: { adapter: "terminal", source: "terminal-adapter" },
      snapshot: true,
    });
    console.log(artifact.id);
    return;
  }

  if (command === "observe") {
    const options = parseObserveArgs(args);
    const artifact = await client.addArtifact(options.sessionId, observeArtifact(options));
    console.log(artifact.id);
    return;
  }

  if (command === "shell-init") {
    const [shell, ...rest] = args;
    const session = optionValue(rest, "--session") ?? "$SESSIONBUS_SESSION";
    process.stdout.write(shellInit(shell, session, process.argv.slice(1).map(shellQuote).join(" ")));
    return;
  }

  throw new Error(
    "usage: terminal-adapter register | capture --session <id> [title...] | observe --session <id> [--shell sh] [--exit-code n] [--duration-ms n] -- <command> | shell-init <zsh|bash|fish> --session <id>",
  );
}

export function observeArtifact(options: ObserveOptions): CreateArtifactRequest {
  const body = [
    `$ ${options.commandLine}`,
    options.exitCode === undefined ? undefined : `exit_code\t${options.exitCode}`,
    options.durationMs === undefined ? undefined : `duration_ms\t${options.durationMs}`,
    options.shell === undefined ? undefined : `shell\t${options.shell}`,
  ]
    .filter(Boolean)
    .join("\n");

  return {
    kind: "tool_invocation",
    title: options.commandLine,
    body,
    metadata: {
      adapter: "terminal",
      source: "terminal-adapter",
      command_line: options.commandLine,
      exit_code: options.exitCode,
      duration_ms: options.durationMs,
      shell: options.shell,
    },
    snapshot: true,
  };
}

export function parseObserveArgs(args: string[]): ObserveOptions {
  const separator = args.indexOf("--");
  const optionArgs = separator >= 0 ? args.slice(0, separator) : args;
  const commandArgs = separator >= 0 ? args.slice(separator + 1) : [];
  const sessionId = optionValue(optionArgs, "--session");
  if (!sessionId) {
    throw new Error("observe requires --session <id>");
  }
  const commandLine = commandArgs.join(" ").trim();
  if (!commandLine) {
    throw new Error("observe requires a command after --");
  }
  return {
    sessionId,
    commandLine,
    exitCode: numberOption(optionArgs, "--exit-code"),
    durationMs: numberOption(optionArgs, "--duration-ms"),
    shell: optionValue(optionArgs, "--shell"),
  };
}

export function shellInit(shell: string | undefined, session: string, adapterCommand: string): string {
  if (shell === "zsh") {
    return `# Sessionbus terminal adapter\nexport SESSIONBUS_SESSION="${session}"\nautoload -Uz add-zsh-hook\n__sessionbus_terminal_preexec() {\n  export __SESSIONBUS_TERMINAL_COMMAND="$1"\n  export __SESSIONBUS_TERMINAL_STARTED_AT="$(date +%s%3N 2>/dev/null || date +%s)"\n}\n__sessionbus_terminal_precmd() {\n  local status="$?"\n  if [[ -n "\${__SESSIONBUS_TERMINAL_COMMAND:-}" && -n "\${SESSIONBUS_SESSION:-}" ]]; then\n    local now="$(date +%s%3N 2>/dev/null || date +%s)"\n    local duration=""\n    if [[ -n "\${__SESSIONBUS_TERMINAL_STARTED_AT:-}" && "$now" == <-> && "$__SESSIONBUS_TERMINAL_STARTED_AT" == <-> ]]; then\n      duration=$(( now - __SESSIONBUS_TERMINAL_STARTED_AT ))\n    fi\n    if [[ -n "$duration" ]]; then\n      command ${adapterCommand} observe --session "$SESSIONBUS_SESSION" --shell zsh --exit-code "$status" --duration-ms "$duration" -- "$__SESSIONBUS_TERMINAL_COMMAND" >/dev/null 2>&1\n    else\n      command ${adapterCommand} observe --session "$SESSIONBUS_SESSION" --shell zsh --exit-code "$status" -- "$__SESSIONBUS_TERMINAL_COMMAND" >/dev/null 2>&1\n    fi\n    unset __SESSIONBUS_TERMINAL_COMMAND __SESSIONBUS_TERMINAL_STARTED_AT\n  fi\n}\nadd-zsh-hook preexec __sessionbus_terminal_preexec\nadd-zsh-hook precmd __sessionbus_terminal_precmd\n`;
  }

  if (shell === "bash") {
    return `# Sessionbus terminal adapter\nexport SESSIONBUS_SESSION="${session}"\n__sessionbus_terminal_prompt_command() {\n  local status="$?"\n  local command_line\n  command_line="$(history 1 | sed 's/^ *[0-9]* *//')"\n  if [[ -n "$command_line" && -n "\${SESSIONBUS_SESSION:-}" && "$command_line" != "$__SESSIONBUS_TERMINAL_COMMAND" ]]; then\n    __SESSIONBUS_TERMINAL_COMMAND="$command_line"\n    command ${adapterCommand} observe --session "$SESSIONBUS_SESSION" --shell bash --exit-code "$status" -- "$command_line" >/dev/null 2>&1\n  fi\n}\nPROMPT_COMMAND="__sessionbus_terminal_prompt_command\${PROMPT_COMMAND:+;$PROMPT_COMMAND}"\n`;
  }

  if (shell === "fish") {
    return `# Sessionbus terminal adapter\nset -gx SESSIONBUS_SESSION "${session}"\nfunction __sessionbus_terminal_postexec --on-event fish_postexec\n  set status $status\n  set command_line (string join " " $argv)\n  if test -n "$command_line"; and test -n "$SESSIONBUS_SESSION"\n    command ${adapterCommand} observe --session "$SESSIONBUS_SESSION" --shell fish --exit-code $status -- "$command_line" >/dev/null 2>&1\n  end\nend\n`;
  }

  throw new Error("shell-init requires zsh, bash, or fish");
}

function requireSession(args: string[], command: string): { sessionId: string; rest: string[] } {
  const sessionIndex = args.indexOf("--session");
  if (sessionIndex < 0 || !args[sessionIndex + 1]) {
    throw new Error(`${command} requires --session <id>`);
  }
  return {
    sessionId: args[sessionIndex + 1],
    rest: args.slice(0, sessionIndex).concat(args.slice(sessionIndex + 2)),
  };
}

function optionValue(args: string[], name: string): string | undefined {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : undefined;
}

function numberOption(args: string[], name: string): number | undefined {
  const value = optionValue(args, name);
  if (value === undefined) {
    return undefined;
  }
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) {
    throw new Error(`${name} must be a number`);
  }
  return parsed;
}

export function shellQuote(value: string): string {
  if (/^[A-Za-z0-9_/:=.,@%+-]+$/.test(value)) {
    return value;
  }
  return `'${value.replaceAll("'", "'\\''")}'`;
}

function readStdin(): Promise<string> {
  return new Promise((resolve, reject) => {
    let data = "";
    process.stdin.setEncoding("utf8");
    process.stdin.on("data", (chunk) => {
      data += chunk;
    });
    process.stdin.on("end", () => resolve(data));
    process.stdin.on("error", reject);
  });
}

if (import.meta.main) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}
