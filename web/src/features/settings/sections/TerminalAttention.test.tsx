import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { request } from "../../../api/client";
import { GeneralSection } from "./GeneralSection";
import { scopedFixture } from "../rows/scopedPreferenceFixture.test-support";
vi.mock("../../../api/client", async (original) => ({ ...(await original<object>()), request: vi.fn() }));

const rows = [
  { key: "terminal_attention_enabled", value: true, default: true, source: "default", project_scoped: false },
  { key: "terminal_attention_quiet_seconds", value: 20, default: 20, source: "default", project_scoped: false },
] as const;

beforeEach(() => {
  vi.mocked(request).mockReset();
  vi.mocked(request).mockResolvedValue(scopedFixture([...rows]));
});

it("shows the two terminal-agent settings in General with the service's values", async () => {
  render(<GeneralSection />);
  expect(await screen.findByText("Terminal agents waiting for you")).toBeVisible();
  const quiet = await screen.findByLabelText("Quiet time before it counts as waiting");
  expect(quiet).toHaveValue(20);
  expect(quiet).toHaveAttribute("min", "10");
  expect(quiet).toHaveAttribute("max", "120");
});

it("saves the switch under its own key", async () => {
  const user = userEvent.setup();
  render(<GeneralSection />);
  await user.click(await screen.findByRole("switch", { name: "Terminal agents waiting for you" }));
  await waitFor(() => expect(request).toHaveBeenCalledWith(
    "PUT", "/api/settings/preferences",
    expect.objectContaining({ edits: { terminal_attention_enabled: { op: "set", value: false } } }),
  ));
});

it.each([["5", 10], ["500", 120], ["45", 45]])("saves the quiet time %s seconds as %i, clamped to 10 to 120", async (typed, saved) => {
  const user = userEvent.setup();
  render(<GeneralSection />);
  const field = await screen.findByLabelText("Quiet time before it counts as waiting");
  await user.clear(field);
  await user.type(field, typed);
  await user.tab();
  await waitFor(() => expect(request).toHaveBeenCalledWith(
    "PUT", "/api/settings/preferences",
    expect.objectContaining({ edits: { terminal_attention_quiet_seconds: { op: "set", value: saved } } }),
  ));
  expect(field).toHaveValue(saved);
});
