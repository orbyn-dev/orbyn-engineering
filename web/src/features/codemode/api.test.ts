import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { codeRun } from "../../../mock/codemode";
import type { MockApi } from "../../../mock/api";
import { ApiFailure } from "../../api/client";
import { installMockFetch } from "../../test-support";
import { getCodeMode, getCodeRun, listCodeRuns, putCodeMode } from "./api";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
});
afterEach(() => vi.restoreAllMocks());

const calls = () => vi.mocked(fetch).mock.calls.map(([url, init]) => `${(init as RequestInit | undefined)?.method ?? "GET"} ${String(url)}`);

describe("the Code Mode client on the service's own routes", () => {
  it("reads a run record from GET /api/code-runs/{id}, the id encoded", async () => {
    mock.codeMode.putRun(codeRun({ run_id: "run_1", status: "completed", started_at: "2026-11-21T10:00:00.000Z", ended_at: "2026-11-21T10:00:01.000Z", calls: [{ index: 1, tool: "t", status: "ok" }] }));
    const record = await getCodeRun("run_1");
    expect(record).toMatchObject({ run_id: "run_1", status: "completed", resumes: 0, truncated: false, crew_worker: false, pending: [], logs: [] });
    await getCodeRun("a b/c");
    expect(calls()).toEqual(["GET /api/code-runs/run_1", "GET /api/code-runs/a%20b%2Fc"]);
  });

  it("lists a thread's runs as CodeRunSummary rows with the generated fields", async () => {
    mock.codeMode.putRun(codeRun({ run_id: "run_old", status: "completed", started_at: "2026-11-21T10:00:00.000Z", ended_at: "2026-11-21T10:00:03.000Z", script: "return 1;", calls: [{ index: 1, tool: "t", status: "ok" }] }));
    mock.codeMode.putRun(codeRun({ run_id: "run_new", status: "running", started_at: "2026-11-21T11:00:00.000Z", script: "x".repeat(300) }));
    const runs = await listCodeRuns("t-1");
    expect(runs?.map((run) => run.run_id)).toEqual(["run_new", "run_old"]);
    expect(runs?.[0]).toEqual({ run_id: "run_new", status: "running", started_at: "2026-11-21T11:00:00.000Z", ended_at: null, calls: 0, script_head: "x".repeat(120), waited_ms: null });
    expect(runs?.[1]).toMatchObject({ status: "completed", ended_at: "2026-11-21T10:00:03.000Z", calls: 1, script_head: "return 1;" });
    expect(calls()).toEqual(["GET /api/threads/t-1/code-runs"]);
  });

  it("reads and writes the engine's switch, one request each", async () => {
    expect(await getCodeMode("claude")).toEqual({ enabled: false });
    expect(await putCodeMode("claude", true)).toEqual({ enabled: true });
    expect(await getCodeMode("claude")).toEqual({ enabled: true });
    expect(calls()).toEqual(["GET /api/engines/claude/code-mode", "PUT /api/engines/claude/code-mode", "GET /api/engines/claude/code-mode"]);
    expect(mock.codeMode.writes).toEqual([{ engine: "claude", enabled: true }]);
  });

  it("answers null on a 404 for every route (Code Mode not offered), and never throws", async () => {
    mock.codeMode.setOffered(false);
    expect(await getCodeRun("run_1")).toBeNull();
    expect(await listCodeRuns("t-1")).toBeNull();
    expect(await getCodeMode("claude")).toBeNull();
    expect(await putCodeMode("claude", true)).toBeNull();
    expect(mock.codeMode.writes).toEqual([]);
  });

  it("answers null for a run that is gone while the service is on", async () => {
    expect(await getCodeRun("run_gone")).toBeNull();
  });

  it("throws every other failure as it is, so a screen can say it", async () => {
    mock.fail("/api/engines", 2, 500);
    await expect(getCodeMode("claude")).rejects.toMatchObject({ status: 500 });
    await expect(putCodeMode("claude", true)).rejects.toBeInstanceOf(ApiFailure);
    mock.fail("/api/code-runs", 1, 503);
    await expect(getCodeRun("run_1")).rejects.toMatchObject({ status: 503 });
  });

  it("reads a reply with no runs key as an empty list", async () => {
    vi.mocked(fetch).mockResolvedValueOnce(new Response("{}", { status: 200 }));
    expect(await listCodeRuns("t-1")).toEqual([]);
  });
});
