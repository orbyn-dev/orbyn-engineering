import type { CodeRunRecord } from "../src/features/codemode/types.ts";
import type { MockResponse } from "./api.ts";

/**
 * MOCK ONLY. The Code Mode routes are not in the service yet (docs/code-mode.md, build steps 3 to 5); this stands in for
 * them with the paths web/src/features/codemode/api.ts guesses at. Nothing here is a contract: when the real routes and their
 * generated types land, delete this file and point api.ts at them. It is dev and test only, like the rest of mock/.
 *
 *   GET /api/code-runs/<id>             one run record, 404 when there is none
 *   GET /api/engines/<id>/code-mode     { enabled }, off until a person turns it on
 *   PUT /api/engines/<id>/code-mode     { enabled } writes it and answers with what is stored
 */
export function createCodeModeMock() {
  let runs: Record<string, CodeRunRecord> = {};
  let enabled: Record<string, boolean> = {};
  let writes: { engine: string; enabled: boolean }[] = [];
  const err = (status: number, code: string, message: string): MockResponse => ({ status, body: { error: { code, message } } });
  return {
    /** The two runs the "codemode" scenario's thread points at. */
    seedDemo() {
      const script = ["const all = await tools.orbyn_thread_list({});", "const failed = [];", "for (const t of all) {", "  const r = await tools.orbyn_thread_read({ thread_id: t.id });", '  if (r.status === "failed") failed.push(t.id);', "}", "console.log(failed.length + ' of ' + all.length + ' failed');", "await tools.orbyn_thread_send({ thread_id: failed[0], prompt: 'Re-run the failing test and report back.' });", "return failed;"].join("\n");
      const read = (index: number, id: string, status: string) => ({ index, tool: "orbyn_thread_read", status: "ok", preview: `{"id":"${id}","status":"${status}","title":"A thread"}`, duration_ms: 200 });
      const calls = [{ index: 1, tool: "orbyn_thread_list", status: "ok", preview: "3 threads", duration_ms: 100 }, read(2, "t_41", "done"), read(3, "t_52", "failed"), read(4, "t_63", "done")];
      const started = new Date(Date.now() - 60_000).toISOString();
      this.putRun({ run_id: "run_done", status: "completed", script, script_sha256: "3f9a1c8d2b7e4f60a1b2c3d4e5f60718293a4b5c6d7e8f9012345678901e07", started_at: started, ended_at: new Date(Date.parse(started) + 3_100).toISOString(), calls: [...calls, { index: 5, tool: "orbyn_thread_send", status: "ok", preview: '{"queued":true}', duration_ms: 300 }], logs: ["1 of 3 failed"], result: ["t_52"] });
      this.putRun({ run_id: "run_paused", status: "paused", script, script_sha256: "3f9a1c8d2b7e4f60a1b2c3d4e5f60718293a4b5c6d7e8f9012345678901e07", started_at: started, ended_at: null, calls: [...calls, { index: 5, tool: "orbyn_thread_send", status: "waiting" }], pending: [{ tool: "orbyn_thread_send", call: 5 }], logs: [] });
    },
    reset() {
      runs = {};
      enabled = {};
      writes = [];
    },
    putRun(record: CodeRunRecord) {
      runs[record.run_id] = record;
    },
    /** Every PUT this mock took, oldest first: a test counts them to prove one click is one write. */
    get writes() {
      return writes;
    },
    handle(method: string, path: string, body?: unknown): MockResponse | null {
      const run = /^\/api\/code-runs\/([^/]+)$/.exec(path);
      if (run && method === "GET") {
        const record = runs[decodeURIComponent(run[1] ?? "")];
        return record ? { status: 200, body: record } : err(404, "not_found", "That run is gone.");
      }
      const mode = /^\/api\/engines\/([^/]+)\/code-mode$/.exec(path);
      if (!mode) return null;
      const engine = decodeURIComponent(mode[1] ?? "");
      if (method === "GET") return { status: 200, body: { enabled: enabled[engine] === true } };
      if (method === "PUT") {
        const next = (body as { enabled?: unknown } | undefined)?.enabled;
        if (typeof next !== "boolean") return err(400, "invalid_input", "Choose on or off.");
        writes.push({ engine, enabled: next });
        enabled[engine] = next;
        return { status: 200, body: { enabled: next } };
      }
      return null;
    },
  };
}
