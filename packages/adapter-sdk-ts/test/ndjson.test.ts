import assert from "node:assert/strict";
import test from "node:test";
import { parseNdjsonStream } from "../src/index.ts";

test("parseNdjsonStream yields records across chunk boundaries", async () => {
  const stream = new ReadableStream<Uint8Array>({
    start(controller) {
      const encoder = new TextEncoder();
      controller.enqueue(encoder.encode('{"id":"evt_1"'));
      controller.enqueue(encoder.encode(',"type":"session.created"}\n{"id":"evt_2","type":"artifact.added"}\n'));
      controller.close();
    },
  });

  const events = [];
  for await (const event of parseNdjsonStream<{ id: string; type: string }>(stream)) {
    events.push(event);
  }

  assert.deepEqual(events, [
    { id: "evt_1", type: "session.created" },
    { id: "evt_2", type: "artifact.added" },
  ]);
});
