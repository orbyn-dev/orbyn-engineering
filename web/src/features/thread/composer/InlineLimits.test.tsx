import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
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

const codexWindows = (used: number) => ({
  engine: "codex", checked_at: "", limits: {}, plan: null,
  windows: [{ id: "primary", label: "5h", used_percent: used, window_minutes: 300, resets_at: null }],
});

it("shows the age and keeps counting it while the panel stays open", async () => {
  vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "Date"] });
  try {
    vi.spyOn(api, "threadLimits").mockResolvedValue(codexWindows(37));
    render(<InlineLimits threadId="age-a" onClose={() => {}} />);
    expect(await screen.findByText("Checked just now")).toBeVisible();
    act(() => { vi.advanceTimersByTime(5 * 60_000); });
    expect(screen.getByText("Checked 5 min ago")).toBeVisible();
  } finally { vi.useRealTimers(); }
});

it("refreshes with force, keeps the lines meanwhile and reads Checking\u2026", async () => {
  let release: (value: ReturnType<typeof codexWindows>) => void = () => {};
  const read = vi.spyOn(api, "threadLimits").mockResolvedValueOnce(codexWindows(37) as never)
    .mockReturnValue(new Promise((resolve) => { release = resolve as never; }));
  render(<InlineLimits threadId="refresh-a" onClose={() => {}} />);
  expect(await screen.findByText("5h: 37% used")).toBeVisible();
  await userEvent.click(screen.getByRole("button", { name: "Refresh limits" }));
  expect(await screen.findByText("Checking\u2026")).toBeVisible();
  expect(screen.getByText("5h: 37% used")).toBeVisible();
  expect(screen.getByRole("button", { name: "Refresh limits" })).toBeDisabled();
  expect(read).toHaveBeenLastCalledWith("refresh-a", expect.any(AbortSignal), true);
  await act(async () => release(codexWindows(41)));
  expect(await screen.findByText("5h: 41% used")).toBeVisible();
});

it("keeps the last lines after a failed refresh and offers Try again", async () => {
  const read = vi.spyOn(api, "threadLimits").mockResolvedValueOnce(codexWindows(37) as never).mockRejectedValue(new Error("down"));
  render(<InlineLimits threadId="fail-a" onClose={() => {}} />);
  expect(await screen.findByText("5h: 37% used")).toBeVisible();
  await userEvent.click(screen.getByRole("button", { name: "Refresh limits" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Couldn\u2019t check limits. Showing the answer from just now.");
  expect(screen.getByText("5h: 37% used")).toBeVisible();
  read.mockResolvedValue(codexWindows(44) as never);
  await userEvent.click(screen.getAllByRole("button", { name: "Try again" })[0]!);
  expect(await screen.findByText("5h: 44% used")).toBeVisible();
  expect(screen.queryByRole("alert")).toBeNull();
});
