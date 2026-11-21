/**
 * Scripted family for protocol and HostDriver tests. The prompt text picks the behaviour:
 * [permission] asks for a tool, [fail] ends the run with a cause chain, [crash] exits the process,
 * [malformed] writes a broken line, [hang] never ends, anything else streams one delta and completes.
 */
export const fake = {
  name: "fake",
  sdk: { name: "fake", version: "0" },
  capabilities: ["steer", "models", "usage", "readHistory", "probe"],
  create(ctx) {
    let nextSession = 0;
    const runs = new Map(); // sendId -> {cancelled, done}
    const pending = new Map(); // requestId -> resolve

    const finish = (sendId, frame) => {
      const run = runs.get(sendId);
      if (!run || run.done) return;
      run.done = true;
      ctx.send({ type: "runDone", sendId, ...frame });
    };

    const play = async (sendId, text) => {
      if (text.includes("[crash]")) process.exit(3);
      if (text.includes("[malformed]")) {
        process.stdout.write("{not json\n");
        return finish(sendId, { ok: true, result: { reason: "completed", threadDisposition: "reusable" } });
      }
      if (text.includes("[hang]")) return;
      if (text.includes("[fail]")) {
        let cause = new Error("root cause");
        for (let i = 0; i < 9; i++) cause = new Error(`level ${i}`, { cause });
        return finish(sendId, { ok: false, error: { method: "send", cause: serializeCause(cause) } });
      }
      if (text.includes("[permission]")) {
        const requestId = `p-${sendId}`;
        const answer = await new Promise((resolve) => {
          pending.set(requestId, resolve);
          ctx.send({ type: "permission", sendId, requestId, tool: "Edit", input: { path: "a.txt" } });
        });
        ctx.send({ type: "delta", sendId, update: { type: "text", text: `permission ${answer}` } });
      } else {
        ctx.send({ type: "delta", sendId, update: { type: "text", text: `echo: ${text}` } });
      }
      const run = runs.get(sendId);
      finish(
        sendId,
        run?.cancelled
          ? { ok: true, result: { reason: "cancelled", threadDisposition: "reusable" } }
          : { ok: true, result: { reason: "completed", threadDisposition: "reusable" } },
      );
    };

    return {
      async handle(request) {
        switch (request.type) {
          case "open":
            return { engineSessionId: `fake-${++nextSession}` };
          case "send": {
            runs.set(request.id, { cancelled: false, done: false });
            setImmediate(() => {
              ctx.send({ type: "session", sendId: request.id, engineSessionId: `fake-${request.sessionId}`, turnId: `turn-${request.id}` });
              void play(request.id, String(request.message));
            });
            return { runId: `run-${request.id}` };
          }
          case "respond": {
            const resolve = pending.get(request.requestId);
            if (!resolve) {
              ctx.log({ type: "lateRespond", requestId: request.requestId });
              return null;
            }
            pending.delete(request.requestId);
            resolve(request.decision);
            return null;
          }
          case "cancel": {
            const run = runs.get(request.sendId);
            if (!run || run.done) return { cancelled: false };
            run.cancelled = true;
            for (const [id, resolve] of pending) {
              pending.delete(id);
              resolve("refused");
            }
            finish(request.sendId, { ok: true, result: { reason: "cancelled", threadDisposition: "reusable" } });
            return { cancelled: true };
          }
          case "steer":
            return { outcome: "accepted", messageId: request.messageId };
          case "probe":
            return { refused: ["write-outside-roots", "read-forbidden"], allowed: [] };
          case "models":
            return [{ id: "fake-1", efforts: ["low", "high"] }];
          case "usage":
            return { inputTokens: 0, outputTokens: 0 };
          case "readHistory":
            return [];
          case "closeSession":
            return null;
          default:
            throw Object.assign(new Error(`unsupported request ${request.type}`), { name: "UnsupportedRequest" });
        }
      },
    };
  },
};

import { serializeError as serializeCause } from "../frame.mjs";
