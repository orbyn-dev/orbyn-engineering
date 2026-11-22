import { describe, expect, it } from "vitest";
import type { BacktestStartView } from "../src/generated/BacktestStartView.ts";
import { createMockApi } from "./api.ts";

/** The start input the mock serves for each state the backtest page draws. */
const view = (scenario: string): BacktestStartView => {
  const api = createMockApi();
  api.reset(scenario as Parameters<typeof api.reset>[0]);
  const answer = api.handle("GET", "/api/crew/backtest/start-view?project=p-app");
  expect(answer.status).toBe(200);
  return answer.body as BacktestStartView;
};

describe("the reviewer backtest start-view mock", () => {
  it("serves a finished last run with three lanes and a budget for each engine", () => {
    const run = view("crew-backtest").run;
    expect(run).not.toBeNull();
    expect(run?.lanes).toHaveLength(3);
    expect(run?.budgets).toHaveLength(3);
    expect(run?.lanes.every((lane) => lane.kind === "review")).toBe(true);
  });

  it("serves a lane whose kind was not recorded", () => {
    expect(view("crew-backtest-kind-none").run?.lanes.map((lane) => lane.kind)).toEqual(["review", "review", null]);
  });

  it("serves three lanes with budgets for only two engines", () => {
    const run = view("crew-backtest-budget-none").run;
    expect(run?.lanes.map((lane) => lane.engine)).toEqual(["claude", "codex", "opencode"]);
    expect(run?.budgets.map((entry) => entry.engine)).toEqual(["claude", "codex"]);
  });

  it("serves no run for a project that never ran a backtest", () => {
    expect(view("crew-backtest-none")).toEqual({ project: "p-app", run: null });
  });

  it("serves a failed last run in any state", () => {
    expect(view("crew-backtest-failed").run?.state).toBe("failed");
    expect(view("crew-backtest-stopped").run?.state).toBe("stopped");
  });

  it("serves a stored run in progress, with no live runner behind it", () => {
    const api = createMockApi();
    api.reset("crew-backtest-running");
    const answer = api.handle("GET", "/api/crew/backtest/estimate?project=p-app");
    expect(answer.body).toMatchObject({ running: "run-live" });
    expect((api.handle("GET", "/api/crew/backtest/start-view?project=p-app").body as BacktestStartView).run?.state).toBe("running");
  });
});
