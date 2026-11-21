import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { AsyncQueue } from "../src/asyncQueue.mjs";

// Replays a T3 opencode2 transcript (MIT, see test/fixtures/NOTICE) as a fake @opencode/client.
// expect_outbound lines are checked against the calls the family makes; emit_inbound events are
// streamed to the event subscriber. The pump runs eagerly and blocks at the next expected call.
// Requests the family never makes on a turn (T3's model.list, instructions entry, decline notes, title
// update) are skipped; server.info and session.get are answered locally, they are not in the record.
// model.list and message.list answer with the recorded response when the family asks for them.
const SKIPPABLE = new Set(["model.list", "agent.list", "session.instructions.entry.put", "session.synthetic", "message.list"]);
const IGNORED_KEYS = new Set(["id", "permissions", "key", "value", "title"]);
const isTitleUpdate = (frame) => frame.type === "session.update" && frame.input && "title" in frame.input;

export function replayOpenCode(file) {
  const lines = readFileSync(file, "utf8").trim().split("\n").map((l) => JSON.parse(l));
  const calls = { ops: [], skipped: [], interrupts: [], replies: [], prompts: [], forks: [], updates: [], forms: [], modelLists: [], messageLists: [], failures: [] };
  const recorded = (label) => lines.find((l) => l.label === `${label}.response`)?.frame.data;
  const events = new AsyncQueue();
  const pending = [];
  let cursor = 0;

  const fail = (call, message) => {
    const error = new Error(message);
    calls.failures.push(message);
    call.reject(error);
  };

  const matches = (frame, op, input) => {
    if (frame.type !== op) return false;
    if (op === "session.update") return isTitleUpdate(frame) === ("title" in (input ?? {}));
    return true;
  };

  const check = (frame, input) => {
    if (!frame.input || typeof frame.input !== "object") return;
    for (const [key, expected] of Object.entries(frame.input)) {
      if (IGNORED_KEYS.has(key)) continue;
      if (expected === "<any>") {
        assert.notEqual(input?.[key], undefined, `${frame.type}.${key} is present`);
        continue;
      }
      assert.deepEqual(input?.[key], expected, `${frame.type}.${key}`);
    }
  };

  function advance() {
    while (cursor < lines.length) {
      const line = lines[cursor];
      if (line.type === "emit_inbound") {
        if (line.frame.type === "sdk.event") events.push(line.frame.event);
        cursor++;
        continue;
      }
      if (line.type !== "expect_outbound") {
        cursor++;
        continue;
      }
      const frame = line.frame;
      if (SKIPPABLE.has(frame.type) || isTitleUpdate(frame)) {
        calls.skipped.push(line.label);
        cursor++;
        continue;
      }
      const call = pending.shift();
      if (!call) return;
      if (!matches(frame, call.op, call.input)) {
        fail(call, `unexpected ${call.op}; the transcript expects ${frame.type} (${line.label})`);
        pending.length = 0;
        return;
      }
      try {
        check(frame, call.input);
      } catch (cause) {
        fail(call, cause.message);
        return;
      }
      const response = lines[cursor + 1];
      cursor += response?.frame?.type === "sdk.response" ? 2 : 1;
      let data = response?.frame?.data;
      if (data && typeof data === "object" && Object.keys(data).length === 1 && "data" in data) data = data.data;
      call.resolve(data ?? undefined);
    }
  }

  const call = (op, record) => (input) =>
    new Promise((resolve, reject) => {
      calls.ops.push(op);
      record?.push(input);
      pending.push({ op, input, resolve, reject });
      advance();
    });

  const client = {
    server: { info: async () => ({ version: "2.0.18" }) },
    session: {
      create: call("session.create"),
      get: async ({ sessionID }) => ({ id: sessionID }),
      prompt: call("session.prompt", calls.prompts),
      interrupt: call("session.interrupt", calls.interrupts),
      fork: call("session.fork", calls.forks),
      update: call("session.update", calls.updates),
      form: { reply: call("session.form.reply", calls.forms) },
    },
    permission: { reply: call("permission.reply", calls.replies) },
    model: {
      list: async (input) => {
        calls.modelLists.push(input);
        return recorded("model.list");
      },
    },
    message: {
      list: async (input) => {
        calls.messageLists.push(input);
        return recorded("message.list");
      },
    },
    event: {
      subscribe(_input, options = {}) {
        calls.ops.push("event.subscribe");
        options.signal?.addEventListener("abort", () => events.close(), { once: true });
        // The first expectation is event.subscribe itself.
        const at = lines.findIndex((l, i) => i >= cursor && l.frame?.type === "event.subscribe");
        assert.ok(at >= 0, "transcript has event.subscribe");
        cursor = at + 1;
        // A real server always opens its stream with server.connected; one recording (nested_background) lacks it.
        if (!lines.some((l) => l.frame?.event?.type === "server.connected")) events.push({ type: "server.connected", data: {} });
        queueMicrotask(advance);
        return events;
      },
    },
  };

  return {
    calls,
    client,
    /** Expectations left that a complete test should have consumed. */
    remaining: () => lines.slice(cursor).filter((l) => l.type === "expect_outbound" && !SKIPPABLE.has(l.frame.type) && !isTitleUpdate(l.frame)).map((l) => l.label),
    /** The recorded response of a read the transcript captured (model.list, message.list), as the SDK returns it. */
    recorded,
    /** The user prompt text recorded in the nth session.prompt response. */
    promptTexts: () => lines.filter((l) => l.label?.startsWith("session.prompt.response")).map((l) => l.frame.data.data.payload.text),
    /** Id of the nth recorded user message, as the server returned it. */
    promptIds: () => lines.filter((l) => l.label?.startsWith("session.prompt.response")).map((l) => l.frame.data.data.id),
  };
}
