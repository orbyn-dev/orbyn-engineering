import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it } from "vitest";
import { installMockFetch } from "../../../test-support";
import { GeneralSection } from "./GeneralSection";
import { sentEdits } from "../rows/preferenceWrites.test-support";

beforeEach(() => {
  installMockFetch();
  window.location.hash = "";
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
  await waitFor(() => expect(sentEdits()).toEqual([{ terminal_attention_enabled: { op: "set", value: false } }]));
});

it.each([["5", 10], ["500", 120], ["45", 45]])("saves the quiet time %s seconds as %i, clamped to 10 to 120", async (typed, saved) => {
  const user = userEvent.setup();
  render(<GeneralSection />);
  const field = await screen.findByLabelText("Quiet time before it counts as waiting");
  await user.clear(field);
  await user.type(field, typed);
  await user.tab();
  await waitFor(() => expect(sentEdits()).toEqual([{ terminal_attention_quiet_seconds: { op: "set", value: saved } }]));
  expect(field).toHaveValue(saved);
});
