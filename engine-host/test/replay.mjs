import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { AsyncQueue } from "../src/asyncQueue.mjs";

/**
 * Replays a T3 claude-agent-sdk.query transcript (MIT, see test/fixtures/NOTICE) as a fake SDK.
 * expect_outbound lines are checked against what the family sends; emit_inbound lines are streamed
 * back. `onInterrupt` resolves when the family interrupts.
 */
export function replaySdk(file) {
  const lines = readFileSync(file, "utf8").trim().split("\n").map((l) => JSON.parse(l));
  const calls = { opens: [], permissionResults: [], interrupts: 0 };
  return {
    calls,
    query({ prompt, options }) {
      let i = lines.findIndex((l) => l.type === "expect_outbound" && l.frame.type === "query.open");
      assert.ok(i >= 0, "transcript has query.open");
      const open = lines[i].frame.options;
      assert.equal(options.permissionMode ?? "default", open.permissionMode ?? "default", "permissionMode");
      calls.opens.push(options);
      i++;
      const out = new AsyncQueue();
      let interruptResolve;
      const interrupted = new Promise((r) => (interruptResolve = r));
      (async () => {
        for await (const message of prompt) {
          const next = lines[i];
          assert.equal(next?.frame?.type, "prompt.offer", `expected prompt.offer at line ${i}`);
          assert.deepEqual(message.message, next.frame.message.message, "prompt content");
          const recorded = lines[i].frame.message.uuid;
          const swap = (frame) => (recorded && message.uuid ? JSON.parse(JSON.stringify(frame).replaceAll(recorded, message.uuid)) : frame);
          i++;
          while (i < lines.length) {
            const l = lines[i];
            if (l.type === "emit_inbound" && l.frame.type === "permission.request") {
              const res = await options.canUseTool(l.frame.toolName, l.frame.input, { signal: new AbortController().signal });
              calls.permissionResults.push(res);
              const exp = lines[i + 1];
              assert.equal(exp.frame.type, "permission.response");
              assert.equal(res.behavior, exp.frame.result.behavior, "permission behavior");
              i += 2;
            } else if (l.type === "emit_inbound") {
              out.push(swap(l.frame));
              i++;
            } else if (l.type === "expect_outbound" && l.frame.type === "query.interrupt") {
              await interrupted;
              i++;
            } else break;
          }
        }
        out.close();
      })().catch((e) => {
        calls.error = e;
        out.close();
      });
      const q = out[Symbol.asyncIterator]();
      return {
        [Symbol.asyncIterator]: () => q,
        interrupt: async () => {
          calls.interrupts++;
          interruptResolve?.();
        },
        close: () => out.close(),
      };
    },
  };
}
