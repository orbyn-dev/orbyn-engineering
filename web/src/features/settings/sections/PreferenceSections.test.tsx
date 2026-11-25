import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useUi } from "../../../product/state/ui";
import { installMockFetch } from "../../../test-support";
import { ChatSection } from "./ChatSection";
import { DiffsSection } from "./DiffsSection";
import { preferenceWrites, sentEdits } from "../rows/preferenceWrites.test-support";

beforeEach(() => {
  installMockFetch();
  window.location.hash = "";
});
afterEach(() => vi.restoreAllMocks());

it("shows Open links from other apps here on a Mac client", async () => {
  vi.spyOn(navigator, "platform", "get").mockReturnValue("MacIntel");
  render(<ChatSection />);
  expect(await screen.findByRole("switch", { name: "Open links from other apps here" })).toBeInTheDocument();
  expect(screen.getByText("macOS links open in the Browser panel")).toBeVisible();
});

it("leaves Open links from other apps here out on a Windows client", async () => {
  vi.spyOn(navigator, "platform", "get").mockReturnValue("Win32");
  render(<ChatSection />);
  // Wait for the preferences to arrive before calling the row absent.
  expect(await screen.findByRole("switch", { name: "Snooze until limits reset" })).toBeInTheDocument();
  expect(screen.queryByRole("switch", { name: "Open links from other apps here" })).toBeNull();
  expect(screen.queryByText("macOS links open in the Browser panel")).toBeNull();
});

it("offers the reference usage-limit defaults with their existing preference keys", async () => {
  const user = userEvent.setup();
  render(<ChatSection />);
  expect(await screen.findByRole("switch", { name: "Snooze until limits reset" })).not.toBeChecked();
  await user.click(screen.getByRole("switch", { name: "Resume after usage limits" }));
  await waitFor(() => expect(sentEdits()).toEqual([{ auto_resume_limited_threads: { op: "set", value: true } }]));
  const [write] = preferenceWrites();
  expect(write?.scope).toEqual({});
  expect(Object.keys(write?.if_match ?? {})).toEqual(["auto_resume_limited_threads"]);
});

it("turns review guides on and sets the guide model", async () => {
  const user = userEvent.setup();
  render(<DiffsSection />);
  await user.click(await screen.findByRole("switch", { name: "Review guides" }));
  await waitFor(() => expect(sentEdits()).toEqual([{ review_guides_enabled: { op: "set", value: true } }]));
  expect(screen.getByLabelText("Guide instructions")).toBeInTheDocument();
  await user.type(screen.getByLabelText("Guide model"), "big-model");
  await user.tab();
  await waitFor(() => expect(sentEdits()).toEqual([
    { review_guides_enabled: { op: "set", value: true } },
    { review_guide_model: { op: "set", value: "big-model" } },
  ]));
});

it("sets the diff view from settings and follows the diff panel's stored layout", async () => {
  const user = userEvent.setup();
  useUi.getState().setPrefs({ diffLayout: "stacked" });
  render(<DiffsSection />);
  const group = await screen.findByRole("radiogroup", { name: "Diff view" });
  expect(within(group).getByRole("radio", { name: "Unified" })).toBeChecked();
  await user.click(within(group).getByRole("radio", { name: "Split" }));
  expect(useUi.getState().diffLayout).toBe("split");
  act(() => useUi.getState().setPrefs({ diffLayout: "stacked" }));
  expect(within(group).getByRole("radio", { name: "Unified" })).toBeChecked();
});
