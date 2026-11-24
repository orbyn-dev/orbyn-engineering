import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { installMockFetch } from "../../../test-support";
import { GeneralSection } from "./GeneralSection";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  window.location.hash = "";
});

const preferenceWrites = () =>
  vi.mocked(fetch).mock.calls.filter(([url, init]) => String(url) === "/api/settings/preferences" && init?.method === "PUT");

const sentBudget = () => {
  const [, init] = preferenceWrites()[0] ?? [];
  const body = JSON.parse(String(init?.body)) as { edits: { crew_turn_budget: { op: string; value: number } } };
  return body.edits.crew_turn_budget;
};

it("shows the crew turn budget in General with 0 as no budget, and saves whole turns", async () => {
  const user = userEvent.setup();
  render(<GeneralSection />);
  const field = await screen.findByLabelText("Crew turn budget");
  expect(field).toHaveValue(0);
  expect(field).toHaveAttribute("min", "0");
  expect(screen.getByText(/Once spent, no new Crew task starts; running tasks finish\. 0 is no budget\./)).toBeVisible();
  await user.clear(field);
  await user.type(field, "12");
  await user.tab();
  await waitFor(() => expect(preferenceWrites()).toHaveLength(1));
  expect(sentBudget()).toEqual({ op: "set", value: 12 });
});

it("keeps the entered turns and shows the service's refusal on its row, and saves nothing", async () => {
  const user = userEvent.setup();
  render(<GeneralSection />);
  const field = await screen.findByLabelText("Crew turn budget");
  await user.clear(field);
  await user.type(field, "12");
  mock.fail("/api/settings/preferences", 1, 409, "settings_conflict", "The crew turn budget would exceed the daily cap.");
  await user.tab();

  const refusal = await screen.findByRole("alert");
  expect(refusal).toHaveTextContent("The crew turn budget would exceed the daily cap.");
  expect(field).toHaveValue(12);
  expect(preferenceWrites()).toHaveLength(1);
  expect(screen.queryByText(/^Saved\b/)).toBeNull();

  const stored = mock.handle("POST", "/api/settings/preferences/query", { scope: {}, keys: ["crew_turn_budget"] });
  const rows = (stored.body as { rows: { key: string; value: unknown }[] }).rows;
  expect(rows.find((row) => row.key === "crew_turn_budget")?.value).toBe(0);
});
