import { createAdapterDescriptor, SessionbusClient } from "@sessionbus/adapter-sdk";

const client = new SessionbusClient();

async function main() {
  const [command, sessionId, ...rest] = process.argv.slice(2);
  if (command === "register") {
    await client.registerAdapter(
      createAdapterDescriptor({
        adapter_id: "sessionbus.terminal",
        protocol: "native-http",
        version: "0.1.0",
        capabilities: ["write_artifact", "stream_updates"],
        metadata: { example: true },
      }),
    );
    console.log("sessionbus.terminal");
    return;
  }

  if (command === "capture" && sessionId) {
    const body = await readStdin();
    const artifact = await client.addArtifact(sessionId, {
      kind: "terminal_output",
      title: rest.join(" ") || "terminal output",
      body,
      metadata: { adapter: "terminal" },
      snapshot: true,
    });
    console.log(artifact.id);
    return;
  }

  throw new Error("usage: terminal-adapter register | capture <session-id> [title...]");
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

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});
