import { expect, it } from "vitest";
import { createMockApi } from "./api";

const readDetails = (scenario: "pulls" | "pulls-rate-limited" | "pulls-limited" | "pulls-limited-still") => {
  const api = createMockApi();
  api.reset(scenario);
  const response = api.handle("POST", "/api/pulls/details", {
    pulls: [205, 203, 142, 57].map((number) => ({ project_id: "p-app", number })),
    fresh: true,
  });
  expect(response.status).toBe(200);
  // The dev server sends JSON: fields withheld with undefined must be absent on the wire.
  return JSON.parse(JSON.stringify(response.body)) as {
    details: { number: number; approval_count?: number; comment_count?: number }[];
    paused: string[];
  };
};

it("supplies the drawn approval count for the standard approved preview", () => {
  const body = readDetails("pulls");
  expect(body.details.find((row) => row.number === 205)).toMatchObject({ approval_count: 1 });
  expect(body.paused).toEqual([]);
});

it("keeps counts unknown for the other detail fixtures", () => {
  const body = readDetails("pulls");
  for (const number of [203, 142, 57]) {
    const row = body.details.find((one) => one.number === number);
    expect(row).toBeDefined();
    expect(row).not.toHaveProperty("approval_count");
  }
});

it("withholds the known approval count and marks it paused when rate limited", () => {
  const body = readDetails("pulls-rate-limited");
  expect(body.details.find((row) => row.number === 205)).toBeDefined();
  for (const row of body.details) {
    expect(row).not.toHaveProperty("approval_count");
    expect(row).not.toHaveProperty("comment_count");
  }
  expect(body.paused).toEqual(["comment_count", "approval_count"]);
});

const limitedReads = (scenario: "pulls-limited" | "pulls-limited-still") => {
  const api = createMockApi();
  api.reset(scenario);
  const read = () =>
    JSON.parse(JSON.stringify(api.handle("POST", "/api/pulls/details", { pulls: [{ project_id: "p-app", number: 205 }, { project_id: "p-app", number: 203 }], fresh: true }).body)) as {
      details: Record<string, unknown>[];
      paused: string[];
      resume_at?: string;
    };
  return [read(), read()] as const;
};

it("answers the first read in full, then pauses every detail field the way the service does and says when it ends", () => {
  const [first, later] = limitedReads("pulls-limited");
  expect(first.paused).toEqual([]);
  expect(first.details.find((row) => row.number === 205)).toHaveProperty("approval_count");
  expect(later.paused).toEqual(["created_at", "requested_reviewers", "viewer_review_state", "comment_count", "approval_count"]);
  expect(later.details.length).toBeGreaterThan(0);
  for (const row of later.details) for (const field of later.paused) expect(row).not.toHaveProperty(field);
  expect(new Date(later.resume_at!).getTime()).toBeGreaterThan(Date.now() + 300_000);
});

it("resumes two seconds from every later read in the still-limited variant, so the next check finds the limit on", () => {
  const [first, later] = limitedReads("pulls-limited-still");
  expect(first.paused).toEqual([]);
  const left = new Date(later.resume_at!).getTime() - Date.now();
  expect(left).toBeGreaterThan(0);
  expect(left).toBeLessThanOrEqual(2_000);
});
