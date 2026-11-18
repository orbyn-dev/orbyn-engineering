import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiFailure } from "../../../../api/client";
import type { BackgroundCrew } from "../../../../generated/BackgroundCrew";
import type { BackgroundRun } from "../../../../generated/BackgroundRun";
import { forgetKept } from "../../../../lib/keptData";
import { FLOW_REFRESH_MS } from "../../flow/live/useFlow";
import { crewApi } from "./api";
import { backgroundView } from "./backgroundModel";
import { useBackground } from "./useBackground";

const budget = { turns: 12, minutes: 9 };
const crew = (state: BackgroundCrew["state"]) => ({ id: "c1", project_id: "p", name: "Fix CI", template: "fix_failing_ci", trigger: { kind: "ci_failure" }, budget, enabled: true, state, reviews_waiting: 0, created_at: "2026-11-01T10:00:00Z", updated_at: "2026-11-01T10:00:00Z" }) as unknown as BackgroundCrew;
const run = (id: string, state: BackgroundRun["state"], at: string) => ({ id, crew_id: "c1", state, turns_spent: 1, minutes_spent: 1, budget, review_waiting: false, started_at: at, updated_at: at }) as BackgroundRun;

let crews: BackgroundCrew[];
let runs: BackgroundRun[];
let crewsRead: ReturnType<typeof vi.spyOn>;
let runsRead: ReturnType<typeof vi.spyOn>;
beforeEach(() => {
  vi.useFakeTimers();
  forgetKept();
  crews = [crew("idle")];
  runs = [];
  crewsRead = vi.spyOn(crewApi, "backgroundCrews").mockImplementation(async () => ({ crews, total: crews.length, next_offset: null }) as never);
  runsRead = vi.spyOn(crewApi, "backgroundRuns").mockImplementation(async () => ({ runs, total: runs.length, next_offset: null }) as never);
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
});

const settle = () => act(async () => { await vi.advanceTimersByTimeAsync(0); });
const tick = () => act(async () => { await vi.advanceTimersByTimeAsync(FLOW_REFRESH_MS); });

describe("Background crews refresh", () => {
  it("shows a run that appears later, with Running, without going back to Loading", async () => {
    const { result } = renderHook(() => useBackground("p"));
    await settle();
    expect(result.current.status).toBe("ready");
    expect(backgroundView(result.current.data!, new Date("2026-11-02T10:00:00Z")).crews[0]!.lastRun).toBeNull();
    crews = [crew("running")];
    runs = [run("r1", "running", "2026-11-02T09:55:00Z")];
    await tick();
    expect(result.current.status).toBe("ready");
    const view = backgroundView(result.current.data!, new Date("2026-11-02T10:00:00Z"));
    expect(view.crews[0]!.status).toBe("Running");
    expect(view.crews[0]!.lastRun).not.toBeNull();
    expect(view.runs).toHaveLength(1);
  });

  it("keeps the list, and shows no error, when a refresh fails", async () => {
    const { result } = renderHook(() => useBackground("p"));
    await settle();
    const before = result.current.data;
    runsRead.mockRejectedValue(new ApiFailure("internal", "The store is busy.", 500));
    await tick();
    expect(result.current.status).toBe("ready");
    expect(result.current.error).toBeNull();
    expect(result.current.data).toBe(before);
    runsRead.mockImplementation(async () => ({ runs: [run("r2", "running", "2026-11-02T09:55:00Z")], total: 1, next_offset: null }) as never);
    await tick();
    expect(result.current.data!.runs["c1"]).toHaveLength(1);
  });

  it("does not start a second read while one is on its way", async () => {
    const { result } = renderHook(() => useBackground("p"));
    await settle();
    const calls = crewsRead.mock.calls.length;
    let release: () => void = () => undefined;
    crewsRead.mockImplementation(() => new Promise((resolve) => { release = () => resolve({ crews, total: 1, next_offset: null } as never); }));
    await tick();
    await tick();
    expect(crewsRead.mock.calls.length).toBe(calls + 1);
    await act(async () => { release(); await vi.advanceTimersByTimeAsync(0); });
    expect(result.current.status).toBe("ready");
  });

  it("stops when the screen goes", async () => {
    const { unmount } = renderHook(() => useBackground("p"));
    await settle();
    const calls = crewsRead.mock.calls.length;
    unmount();
    await tick();
    await tick();
    expect(crewsRead.mock.calls.length).toBe(calls);
  });
});
