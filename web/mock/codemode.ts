import type { CodeRunRecord } from "../src/features/codemode/types.ts";
import type { MockResponse } from "./api.ts";

/**
 * A run record with every field the generated `CodeRunRecord` always carries filled in, so a test or the demo names only what it
 * means to show.
 */
export function codeRun(record: Pick<CodeRunRecord, "run_id" | "status" | "started_at"> & Partial<CodeRunRecord>): CodeRunRecord {
  return { calls: [], logs: [], pending: [], truncated: false, resumes: 0, crew_worker: false, ...record };
}

/**
 * DEV AND TEST ONLY (`#/preview`, the "codemode" scenario, vitest): this stands in for the service's Code Mode routes
 * (docs/code-mode.md, "As built: run record, routes and the switch") with the generated `CodeRunRecord`, `CodeRunSummary` and
 * `CodeModeSetting` shapes. The real client (web/src/features/codemode/api.ts) never imports it; web/mock is not in a production
 * build. With `setOffered(false)` every route answers 404, as the service does when `ORBYN_CODE_MODE` is off.
 *
 *   GET /api/code-runs/<id>             one run record, 404 when there is none
 *   GET /api/engines/<id>/code-mode     { enabled }, off until a person turns it on
 *   PUT /api/engines/<id>/code-mode     { enabled } writes it and answers with what is stored
 *   GET /api/threads/<id>/code-runs     { runs } one summary per stored run, newest first
 */
export function createCodeModeMock() {
  let runs = new Map<string, CodeRunRecord>();
  let enabled = new Map<string, boolean>();
  let writes: { engine: string; enabled: boolean }[] = [];
  let offered = true;
  const err = (status: number, code: string, message: string): MockResponse => ({ status, body: { code, message } });
  return {
    /** The two runs the "codemode" scenario's thread points at. */
    seedDemo() {
      const script = ["const all = await tools.orbyn_thread_list({});", "const failed = [];", "for (const t of all) {", "  const r = await tools.orbyn_thread_read({ thread_id: t.id });", '  if (r.status === "failed") failed.push(t.id);', "}", "console.log(failed.length + ' of ' + all.length + ' failed');", "await tools.orbyn_thread_send({ thread_id: failed[0], prompt: 'Re-run the failing test and report back.' });", "return failed;"].join("\n");
      const read = (index: number, id: string, status: string) => ({ index, tool: "orbyn_thread_read", status: "ok", preview: `{"id":"${id}","status":"${status}","title":"A thread"}`, duration_ms: 200 });
      const calls = [{ index: 1, tool: "orbyn_thread_list", status: "ok", preview: "3 threads", duration_ms: 100 }, read(2, "t_41", "done"), read(3, "t_52", "failed"), read(4, "t_63", "done")];
      const started = new Date(Date.now() - 60_000).toISOString();
      this.putRun(codeRun({ run_id: "run_done", status: "completed", script, script_sha256: "3f9a1c8d2b7e4f60a1b2c3d4e5f60718293a4b5c6d7e8f9012345678901e07", started_at: started, ended_at: new Date(Date.parse(started) + 3_100).toISOString(), calls: [...calls, { index: 5, tool: "orbyn_thread_send", status: "ok", preview: '{"queued":true}', duration_ms: 300 }], logs: ["1 of 3 failed"], result: ["t_52"] }));
      this.putRun(codeRun({ run_id: "run_paused", status: "paused", script, script_sha256: "3f9a1c8d2b7e4f60a1b2c3d4e5f60718293a4b5c6d7e8f9012345678901e07", started_at: started, ended_at: null, calls: [...calls, { index: 5, tool: "orbyn_project_create", status: "running" }], pending: [{ tool: "orbyn_project_create", call: 5 }], logs: [] }));
    },
    reset() {
      runs = new Map();
      enabled = new Map();
      writes = [];
      offered = true;
    },
    /** `false` makes every Code Mode route answer 404, as the service does with Code Mode off. */
    setOffered(next: boolean) {
      offered = next;
    },
    putRun(record: CodeRunRecord) {
      runs.set(record.run_id, record);
    },
    /** Every PUT this mock took, oldest first: a test counts them to prove one click is one write. */
    get writes() {
      return writes;
    },
    handle(method: string, path: string, body?: unknown): MockResponse | null {
      if (!offered) return err(404, "not_found", "Not found.");
      const run = /^\/api\/code-runs\/([^/]+)$/.exec(path);
      if (run && method === "GET") {
        const record = runs.get(decodeURIComponent(run[1] ?? ""));
        return record ? { status: 200, body: record } : err(404, "not_found", "That run is gone.");
      }
      const list = /^\/api\/threads\/([^/]+)\/code-runs$/.exec(path);
      if (list && method === "GET") {
        const summaries = [...runs.values()].sort((a, b) => Date.parse(b.started_at) - Date.parse(a.started_at)).map((record) => ({
          run_id: record.run_id, status: record.status, started_at: record.started_at, ended_at: record.ended_at ?? null, calls: record.calls.length,
          script_head: record.script ? record.script.slice(0, 120) : null,
        }));
        return { status: 200, body: { runs: summaries } };
      }
      const mode = /^\/api\/engines\/([^/]+)\/code-mode$/.exec(path);
      if (!mode) return null;
      const engine = decodeURIComponent(mode[1] ?? "");
      if (method === "GET") return { status: 200, body: { enabled: enabled.get(engine) === true } };
      if (method === "PUT") {
        const next = (body as { enabled?: unknown } | undefined)?.enabled;
        if (typeof next !== "boolean") return err(400, "invalid_input", "Choose on or off.");
        writes.push({ engine, enabled: next });
        enabled.set(engine, next);
        return { status: 200, body: { enabled: next } };
      }
      return null;
    },
  };
}
