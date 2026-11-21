import { describe, expect, it } from "vitest";
import { PERIODS } from "../src/features/crew/dashboard/contract.ts";
import { SAMPLE_PROJECTS, createCrewDashboardMock, sampleHistory } from "./crewDashboard.ts";

describe("the Crew dashboard sample client", () => {
  it("answers the 60-day sample the designs draw", () => {
    const sample = sampleHistory("orbyn-engineering", 60);
    expect([sample.merges, sample.unaided, sample.reverted]).toEqual([38, 27, 2]);
    expect(sample.weeks.reduce((sum, week) => sum + week.merges, 0)).toBe(38);
    expect(sample.engines.reduce((sum, row) => sum + row.merges, 0)).toBe(38);
    expect(sample.areas.reduce((sum, row) => sum + row.merges, 0)).toBe(38);
  });

  it.each(SAMPLE_PROJECTS.flatMap((project) => PERIODS.map((days) => [project.id, days] as const)))("%s over %i days never has more reverts or unaided merges than merges", (id, days) => {
    const sample = sampleHistory(id, days);
    expect(sample.days).toBe(days);
    expect(sample.reverted).toBeLessThanOrEqual(sample.merges);
    expect(sample.unaided).toBeLessThanOrEqual(sample.merges);
    expect(sample.reverted_auto + sample.reverted_by_person).toBeLessThanOrEqual(sample.reverted);
    for (const week of sample.weeks) expect(week.reverted + 0).toBeLessThanOrEqual(week.merges);
  });

  it("refuses a project it does not know in the service's words, and reads __proto__ as unknown", () => {
    expect(() => sampleHistory("nope", 60)).toThrow(expect.objectContaining({ code: "not_found", status: 404 }));
    expect(() => sampleHistory("__proto__", 60)).toThrow(expect.objectContaining({ code: "not_found" }));
  });

  it("records every read, fails when told to, and can be built without the history read", async () => {
    const mock = createCrewDashboardMock();
    const signal = new AbortController().signal;
    await mock.history!({ project: "orbyn-docs", days: 30 }, signal);
    await mock.trust("orbyn-docs", signal);
    mock.failNext();
    await expect(mock.thresholds(signal)).rejects.toMatchObject({ code: "history_unreadable" });
    expect(mock.reads).toEqual(["history:orbyn-docs:30", "trust:orbyn-docs", "thresholds"]);
    expect(createCrewDashboardMock({ withHistory: false }).history).toBeNull();
  });
});
