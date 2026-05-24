import { readFile } from "node:fs/promises";
import { basename, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { createAdapterDescriptor, SessionbusClient } from "@sessionbus/adapter-sdk";

const client = new SessionbusClient();

async function main() {
  const [command, sessionId, filePath] = process.argv.slice(2);
  if (command === "register") {
    await client.registerAdapter(
      createAdapterDescriptor({
        adapter_id: "sessionbus.filesystem",
        protocol: "filesystem",
        version: "0.1.0",
        capabilities: ["read_workspace", "write_artifact"],
        metadata: { example: true },
      }),
    );
    console.log("sessionbus.filesystem");
    return;
  }

  if (command === "add-file" && sessionId && filePath) {
    const absolute = resolve(filePath);
    const body = await readFile(absolute, "utf8");
    const artifact = await client.addArtifact(sessionId, {
      kind: "file",
      title: basename(absolute),
      uri: pathToFileURL(absolute).toString(),
      body,
      metadata: { path: absolute, adapter: "filesystem" },
      snapshot: true,
    });
    console.log(artifact.id);
    return;
  }

  throw new Error("usage: filesystem-adapter register | add-file <session-id> <path>");
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});
