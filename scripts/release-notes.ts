#!/usr/bin/env bun
import { readFile } from "node:fs/promises";

const version = normalizeVersion(process.argv[2] ?? (await workspaceVersion()));
const date = new Date().toISOString().slice(0, 10);

console.log(`# Sessionbus ${version}`);
console.log();
console.log(`Release date: ${date}`);
console.log();
console.log("## What ships");
console.log();
console.log("- Local-first Rust daemon and `aictx` CLI backed by SQLite.");
console.log("- Durable sessions, artifacts, decisions, coordination messages, and deterministic context packs.");
console.log("- MCP bridge via `aictx mcp --ensure-daemon` for AI tools that can call MCP.");
console.log("- Dashboard for sessions, recent artifacts, integration health, and pack rendering.");
console.log("- Opt-in shell capture and Bun sidecar adapters for terminal/filesystem workflows.");
console.log("- `aictx setup` bootstrap flow, shell completions, and local source install script.");
console.log();
console.log("## Try it");
console.log();
console.log("```bash");
console.log("cargo build --workspace");
console.log("target/debug/aictx setup");
console.log("target/debug/aictx start --repo \"Fix flaky deploy\"");
console.log("target/debug/aictx note \"Issue only happens in staging\"");
console.log("target/debug/aictx pack --for cursor");
console.log("```");
console.log();
console.log("## Install from source");
console.log();
console.log("```bash");
console.log("PREFIX=\"$HOME/.local\" ./scripts/install.sh");
console.log("export PATH=\"$HOME/.local/bin:$PATH\"");
console.log("aictx setup --write --auto-capture --open-dashboard");
console.log("```");
console.log();
console.log("## Privacy boundary");
console.log();
console.log("- Loopback daemon by default.");
console.log("- Per-user local SQLite store.");
console.log("- No cloud sync in v0.");
console.log("- Explicit file/output capture and redaction-before-pack rendering.");
console.log();
console.log("## Verification");
console.log();
console.log("```bash");
console.log("bun run selftest");
console.log("```");

async function workspaceVersion(): Promise<string> {
  const cargoToml = await readFile("Cargo.toml", "utf8");
  const match = cargoToml.match(/^\s*version\s*=\s*"([^"]+)"/m);
  if (!match) {
    throw new Error("could not find workspace version in Cargo.toml");
  }
  return match[1];
}

function normalizeVersion(value: string): string {
  const trimmed = value.trim();
  return trimmed.startsWith("v") ? trimmed : `v${trimmed}`;
}
