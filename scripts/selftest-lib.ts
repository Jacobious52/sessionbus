export type StepStatus = "pass" | "fail" | "skip";

export type StepResult = {
  name: string;
  status: StepStatus;
  durationMs: number;
  command?: string;
  exitCode?: number;
  stdout?: string;
  stderr?: string;
  likelyCause?: string;
  suggestedNextAction?: string;
};

export type SelftestReport = {
  status: "pass" | "fail";
  startedAt: string;
  finishedAt: string;
  durationMs: number;
  summary: string;
  failedStep?: StepResult;
  steps: StepResult[];
};

export type FailureInput = {
  command?: string;
  exitCode?: number;
  stdout?: string;
  stderr?: string;
};

const SECRET_ASSIGNMENT =
  /\b([A-Z0-9_]*(?:API_KEY|AUTH_TOKEN|PASSWORD|PRIVATE_KEY|SECRET|TOKEN)[A-Z0-9_]*\s*=\s*)([^\s]+)/gi;

export function commandDisplay(command: string, args: string[] = []): string {
  return [command, ...args].map(quoteShellArg).join(" ");
}

export function quoteShellArg(value: string): string {
  if (/^[A-Za-z0-9_./:@%+=,-]+$/.test(value)) {
    return value;
  }
  return `"${value.replaceAll("\\", "\\\\").replaceAll('"', '\\"')}"`;
}

export function redactExcerpt(value = "", maxLength = 4_000): string {
  const redacted = value.replace(SECRET_ASSIGNMENT, "$1[REDACTED]");
  if (redacted.length <= maxLength) {
    return redacted;
  }
  return `${redacted.slice(0, maxLength)}\n...[truncated ${redacted.length - maxLength} chars]`;
}

export function assertIncludes(haystack: string, needle: string, label: string): void {
  if (!haystack.includes(needle)) {
    throw new Error(`${label}: expected output to include ${JSON.stringify(needle)}`);
  }
}

export function assertExcludes(haystack: string, needle: string, label: string): void {
  if (haystack.includes(needle)) {
    throw new Error(`${label}: expected output not to include ${JSON.stringify(needle)}`);
  }
}

export function summarizeFailure(input: FailureInput): Pick<
  StepResult,
  "likelyCause" | "suggestedNextAction"
> {
  const combined = `${input.command ?? ""}\n${input.stdout ?? ""}\n${input.stderr ?? ""}`;
  const lower = combined.toLowerCase();

  if (lower.includes("command not found: bun") || lower.includes("bun: command not found")) {
    return {
      likelyCause: "Bun is not installed or not on PATH.",
      suggestedNextAction:
        "Run the bun install command from https://bun.sh, then rerun `npm run selftest` or `bun run selftest`.",
    };
  }

  if (
    lower.includes("command not found: cargo") ||
    lower.includes("cargo: command not found") ||
    lower.includes("rustc: command not found") ||
    lower.includes('executable not found in $path: "cargo"') ||
    lower.includes('executable not found in $path: "rustc"')
  ) {
    return {
      likelyCause: "Rust toolchain is not installed or cargo is not on PATH.",
      suggestedNextAction: "Install Rust with rustup, then rerun the selftest.",
    };
  }

  if (lower.includes("address already in use") || lower.includes("eaddrinuse")) {
    return {
      likelyCause: "The selected local daemon port is already in use.",
      suggestedNextAction: "Rerun the selftest; it chooses a fresh port each time.",
    };
  }

  if (lower.includes("connection refused") || lower.includes("failed to fetch")) {
    return {
      likelyCause: "The daemon did not become reachable before the harness timed out.",
      suggestedNextAction: "Inspect daemon stderr in `selftest-report.json`, fix the startup error, and rerun.",
    };
  }

  if (
    lower.includes("failed to listen at 127.0.0.1") ||
    lower.includes("listen eperm") ||
    lower.includes("operation not permitted 127.0.0.1")
  ) {
    return {
      likelyCause: "Localhost binding is blocked by the current sandbox or environment.",
      suggestedNextAction:
        "Rerun `npm run selftest` with local network permissions, or run it in a normal developer shell.",
    };
  }

  return {
    likelyCause: "A selftest command failed.",
    suggestedNextAction: "Inspect stdout/stderr in `selftest-report.json`, fix the failing step, and rerun.",
  };
}

export function failedStepFromError(name: string, error: unknown, durationMs: number): StepResult {
  const message = error instanceof Error ? error.message : String(error);
  return {
    name,
    status: "fail",
    durationMs,
    stderr: redactExcerpt(message),
    ...summarizeFailure({ stderr: message }),
  };
}

export function buildReport(
  steps: StepResult[],
  startedAt = new Date(),
  finishedAt = new Date(),
): SelftestReport {
  const failedStep = steps.find((step) => step.status === "fail");
  const status = failedStep ? "fail" : "pass";
  return {
    status,
    startedAt: startedAt.toISOString(),
    finishedAt: finishedAt.toISOString(),
    durationMs: Math.max(0, finishedAt.getTime() - startedAt.getTime()),
    summary: failedStep
      ? `Selftest failed at "${failedStep.name}".`
      : `Selftest passed ${steps.length} steps.`,
    failedStep,
    steps,
  };
}
