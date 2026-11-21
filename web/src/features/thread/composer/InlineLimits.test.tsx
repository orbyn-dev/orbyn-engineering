import { act, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { api } from "../api";
import { InlineLimits } from "./ComposerExtras";
import { dataChanged } from "../../../api/shell";
import { startDataRefresh } from "../../../lib/dataRefresh";
afterEach(() => vi.restoreAllMocks());

it("drops the previous account's inline windows when an event read fails", async () => {
  const stop = startDataRefresh();
  try {
    const read = vi.spyOn(api, "threadLimits").mockResolvedValueOnce({
      engine: "codex", checked_at: "", limits: {}, plan: null,
      windows: [{ id: "primary", label: "5h", used_percent: 37, window_minutes: 300, resets_at: null }],
    }).mockRejectedValue(new Error("new account unavailable"));
    render(<InlineLimits threadId="account-switch" onClose={() => {}} />);
    expect(await screen.findByText("5h: 37% used")).toBeVisible();
    act(() => { dataChanged("limits"); });
    await waitFor(() => expect(read).toHaveBeenCalledTimes(2));
    expect(screen.queryByText("5h: 37% used")).toBeNull();
    expect(screen.getByText(/Couldn't read limits/)).toBeVisible();
  } finally { stop(); }
});

it("serves a second open of the panel from the five-minute window instead of asking again", async () => {
  const read = vi.spyOn(api, "threadLimits").mockResolvedValue({
    engine: "codex", checked_at: "", limits: {}, plan: null,
    windows: [{ id: "primary", label: "5h", used_percent: 37, window_minutes: 300, resets_at: null }],
  });
  const first = render(<InlineLimits threadId="window-a" onClose={() => {}} />);
  expect(await screen.findByText("5h: 37% used")).toBeVisible();
  first.unmount();
  render(<InlineLimits threadId="window-a" onClose={() => {}} />);
  expect(await screen.findByText("5h: 37% used")).toBeVisible();
  expect(read).toHaveBeenCalledTimes(1);
});
