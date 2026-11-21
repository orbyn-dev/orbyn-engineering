import { expect, it } from "vitest";
import { createMockApi } from "./api";

const readDetails = (scenario: "pulls" | "pulls-rate-limited") => {
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
