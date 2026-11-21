import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { request } from "../../../api/client";
import { GeneralSection } from "./GeneralSection";
import { scopedFixture } from "../rows/scopedPreferenceFixture.test-support";
vi.mock("../../../api/client", async (original) => ({ ...(await original<object>()), request: vi.fn() }));

beforeEach(() => {
  vi.mocked(request).mockReset();
});

it("shows the crew turn budget in General with 0 as no budget, and saves whole turns", async () => {
  const user = userEvent.setup();
  vi.mocked(request).mockResolvedValue(scopedFixture([
    { key: "crew_turn_budget", value: 0, default: 0, source: "default", project_scoped: false },
  ]));
  render(<GeneralSection />);
  const field = await screen.findByLabelText("Crew turn budget");
  expect(field).toHaveValue(0);
  expect(field).toHaveAttribute("min", "0");
  expect(screen.getByText(/Once spent, no new Crew task starts; running tasks finish\. 0 is no budget\./)).toBeVisible();
  await user.clear(field);
  await user.type(field, "12");
  await user.tab();
  await waitFor(() => expect(request).toHaveBeenCalledWith(
    "PUT",
    "/api/settings/preferences",
    expect.objectContaining({ edits: { crew_turn_budget: { op: "set", value: 12 } } }),
  ));
});
