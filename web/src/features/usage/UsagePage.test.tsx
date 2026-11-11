import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { api, ApiFailure } from "../../api/client";
import type { UsageReport } from "../../api/types";
import { useWorkspace } from "../../product/state/workspace";
import { forgetLastLimits, LIMITS_WAIT_MS } from "./UsageLimits";
import { UsagePage } from "./UsagePage";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

function report(tokens: number, model = "local-model"): UsageReport {
  return {
    host_id: "local", scanned_at: "2026-11-05T00:00:00Z",
    engines: [{ engine: "claude", status: "scanned", files_scanned: 1, files_skipped: 0,
      models: [{ model, input_tokens: tokens, output_tokens: 0, cache_read_tokens: 0,
        cache_write_tokens: 0, estimated_cost: null, cost_split: null,
        tiers: [{ tier: "standard", tokens, estimated_cost: null }], unpriced_tokens: tokens, days: [] }], days: [] }],
  };
}

const row = () => screen.getByRole("row", { name: /local-model/ });

beforeEach(() => {
  vi.restoreAllMocks();
  vi.spyOn(api, "usage").mockResolvedValue(report(101));
});

it("keeps the same period mounted and uses the refresh response without another read", async () => {
  const refresh = deferred<UsageReport>();
  vi.spyOn(api, "refreshUsage").mockReturnValue(refresh.promise);
  render(<UsagePage />);
  expect(await screen.findByRole("row", { name: /local-model/ })).toHaveTextContent("101");
  const original = row();
  await userEvent.click(screen.getByRole("button", { name: "Refresh usage" }));
  expect(row()).toBe(original);
  expect(screen.getByRole("status")).toHaveTextContent("Refreshing usage");
  await act(async () => refresh.resolve(report(202)));
  expect(row()).toBe(original);
  expect(row()).toHaveTextContent("202");
  expect(api.usage).toHaveBeenCalledTimes(1);
  expect(screen.getByRole("button", { name: "Refresh usage" })).toBeEnabled();
});

it("retains unpriced figures on refresh failure and retries the refresh", async () => {
  const refresh = deferred<UsageReport>();
  vi.spyOn(api, "refreshUsage").mockReturnValueOnce(refresh.promise).mockResolvedValue(report(303));
  render(<UsagePage />);
  await screen.findByRole("row", { name: /local-model/ });
  await userEvent.click(screen.getByRole("button", { name: "Refresh usage" }));
  await act(async () => refresh.reject(new ApiFailure("unavailable", "Local scan failed.", 503)));
  expect(row()).toHaveTextContent("101");
  expect(row()).toHaveTextContent("Unpriced");
  expect(screen.getByRole("alert")).toHaveTextContent("Local scan failed.");
  await userEvent.click(within(screen.getByRole("alert")).getByRole("button", { name: "Retry" }));
  await waitFor(() => expect(row()).toHaveTextContent("303"));
  expect(api.refreshUsage).toHaveBeenCalledTimes(2);
  expect(screen.queryByRole("alert")).toBeNull();
});

it.each(["success", "failure"])("rejects an old period refresh %s without clearing the new pending read", async (outcome) => {
  const refresh = deferred<UsageReport>();
  const week = deferred<UsageReport>();
  vi.spyOn(api, "refreshUsage").mockReturnValue(refresh.promise);
  vi.mocked(api.usage).mockImplementation((days) => days === 7 ? week.promise : Promise.resolve(report(101)));
  render(<UsagePage />);
  await screen.findByRole("row", { name: /local-model/ });
  await userEvent.click(screen.getByRole("button", { name: "Refresh usage" }));
  await userEvent.click(screen.getByRole("radio", { name: "7 days" }));
  expect(screen.queryByRole("row", { name: /local-model/ })).toBeNull();
  await act(async () => {
    if (outcome === "success") refresh.resolve(report(999));
    else refresh.reject(new ApiFailure("unavailable", "Old period error.", 503));
  });
  expect(screen.queryByRole("row", { name: /local-model/ })).toBeNull();
  expect(screen.queryByRole("alert")).toBeNull();
  expect(screen.getByRole("button", { name: "Refresh usage" })).toBeDisabled();
  await act(async () => week.resolve(report(707)));
  expect(row()).toHaveTextContent("707");
  expect(screen.getByRole("button", { name: "Refresh usage" })).toBeEnabled();
});

it("retries an initial read without pretending a previous report exists", async () => {
  vi.mocked(api.usage).mockRejectedValueOnce(new ApiFailure("unavailable", "History unavailable.", 503))
    .mockResolvedValue(report(404));
  render(<UsagePage />);
  expect(await screen.findByRole("alert")).toHaveTextContent("History unavailable.");
  expect(screen.queryByRole("table")).toBeNull();
  await userEvent.click(screen.getByRole("button", { name: "Retry" }));
  expect(await screen.findByRole("row", { name: /local-model/ })).toHaveTextContent("404");
});

it("does not revive a refresh after leaving and returning to its period", async () => {
  const old = deferred<UsageReport>();
  vi.spyOn(api, "refreshUsage").mockReturnValue(old.promise);
  vi.mocked(api.usage).mockResolvedValueOnce(report(101)).mockResolvedValueOnce(report(707))
    .mockResolvedValueOnce(report(303));
  render(<UsagePage />);
  await screen.findByRole("row", { name: /local-model/ });
  await userEvent.click(screen.getByRole("button", { name: "Refresh usage" }));
  await userEvent.click(screen.getByRole("radio", { name: "7 days" }));
  await waitFor(() => expect(row()).toHaveTextContent("707"));
  await userEvent.click(screen.getByRole("radio", { name: "30 days" }));
  await waitFor(() => expect(row()).toHaveTextContent("303"));
  await act(async () => old.resolve(report(999)));
  expect(row()).toHaveTextContent("303");
});

it("keeps provider selection when the same-period report refreshes", async () => {
  const both = (tokens: number): UsageReport => ({ ...report(tokens),
    engines: [...report(tokens).engines, { ...report(888, "second-model").engines[0]!, engine: "codex" }],
  });
  vi.mocked(api.usage).mockResolvedValue(both(101));
  vi.spyOn(api, "refreshUsage").mockResolvedValue(both(202));
  render(<UsagePage />);
  await screen.findByRole("row", { name: /local-model/ });
  await userEvent.click(screen.getByRole("button", { name: "All providers" }));
  await userEvent.click(screen.getByRole("menuitem", { name: "claude" }));
  expect(screen.queryByRole("row", { name: /second-model/ })).toBeNull();
  await userEvent.click(screen.getByRole("button", { name: "Refresh usage" }));
  await waitFor(() => expect(row()).toHaveTextContent("202"));
  expect(screen.queryByRole("row", { name: /second-model/ })).toBeNull();
  expect(screen.getByRole("button", { name: "claude" })).toBeVisible();
});

it("splits cost by token type and tier and keeps an unpriced tier unknown", async () => {
  const priced = report(300);
  priced.engines[0]!.models[0] = {
    ...priced.engines[0]!.models[0]!, estimated_cost: 18,
    cost_split: { input: 7, output: 10, cache_read: 1, cache_write: 0 },
    tiers: [
      { tier: "standard", tokens: 200, estimated_cost: 12 },
      { tier: "fast", tokens: 50, estimated_cost: 6 },
      { tier: "priority", tokens: 50, estimated_cost: null },
    ],
    unpriced_tokens: 50,
  };
  vi.mocked(api.usage).mockResolvedValue(priced);
  render(<UsagePage />);
  await screen.findByRole("row", { name: /local-model/ });
  await userEvent.click(screen.getByRole("radio", { name: "Cost" }));
  const split = screen.getByRole("region", { name: "Cost split" });
  expect(split).toHaveTextContent("Output$10.00");
  expect(split).toHaveTextContent("Fast · 50 tokens$6.00");
  expect(split).toHaveTextContent("Priority · 50 tokensUnpriced");
  expect(split).toHaveTextContent("Standard · 200 tokens$12.00");
  expect(within(split).getByRole("note")).toHaveTextContent("covers the standard tier only");
});

it("shows each model's cache hit, cost per million and trend", async () => {
  const today = new Date().toISOString().slice(0, 10);
  const data = report(1_000_000);
  data.engines[0]!.models[0] = {
    ...data.engines[0]!.models[0]!, input_tokens: 500_000, cache_read_tokens: 500_000,
    estimated_cost: 3, cost_split: { input: 3, output: 0, cache_read: 0, cache_write: 0 },
    tiers: [{ tier: "standard", tokens: 1_000_000, estimated_cost: 3 }], unpriced_tokens: 0,
    days: [{ date: today, tokens: 1_000_000, estimated_cost: 3 }],
  };
  vi.mocked(api.usage).mockResolvedValue(data);
  render(<UsagePage />);
  const line = await screen.findByRole("row", { name: /local-model/ });
  expect(line).toHaveTextContent("50.0%");
  expect(line).toHaveTextContent("$3.00/M");
  expect(within(line).getByRole("img", { name: "local-model daily tokens" })).toBeInTheDocument();
});

it("draws a provider share bar and a daily cost chart for the period", async () => {
  const today = new Date().toISOString().slice(0, 10);
  const data = report(300);
  data.engines[0]!.models[0] = { ...data.engines[0]!.models[0]!, estimated_cost: 3, unpriced_tokens: 0 };
  data.engines[0]!.days = [{ date: today, tokens: 300, estimated_cost: 3 }];
  data.engines.push({ ...report(100, "second-model").engines[0]!, engine: "codex",
    days: [{ date: today, tokens: 100, estimated_cost: null }] });
  vi.mocked(api.usage).mockResolvedValue(data);
  render(<UsagePage />);
  await screen.findByRole("row", { name: /local-model/ });
  const tokens = screen.getByRole("group", { name: "Share of tokens" });
  expect(within(tokens).getByLabelText("claude 75.0%")).toBeInTheDocument();
  expect(within(tokens).getByLabelText("codex 25.0%")).toBeInTheDocument();
  await userEvent.click(screen.getByRole("radio", { name: "Cost" }));
  expect(within(screen.getByRole("group", { name: "Share of priced cost" })).getByLabelText("claude 100.0%")).toBeInTheDocument();
  expect(screen.getByRole("img", { name: "Daily estimated cost by engine" })).toBeInTheDocument();
});

it("says what the local scan read, including engines with no history and unreadable files", async () => {
  const data = report(5);
  data.engines[0] = { ...data.engines[0]!, files_scanned: 3, files_skipped: 1 };
  data.engines.push({ engine: "codex", status: "no_history", files_scanned: 0, files_skipped: 0, models: [], days: [] });
  vi.mocked(api.usage).mockResolvedValue(data);
  render(<UsagePage />);
  const status = await screen.findByLabelText("Scan status");
  expect(status).toHaveTextContent("claude: 3 files, 1 unreadable");
  expect(status).toHaveTextContent("codex: no local history");
});

const engine = (id: string, name: string, over: Record<string, unknown> = {}) =>
  ({ id, name, installed: true, auth: "logged_in", ...over }) as never;
const codexWindow = (over: Record<string, unknown> = {}) =>
  ({ id: "primary", label: "5 hours", used_percent: 85, window_minutes: 300, resets_at: null, ...over });
const limitsOf = (engines: unknown[], unavailable: unknown[] = []) => ({ pooled: [], engines, unavailable }) as never;
const openLimits = async () => {
  render(<UsagePage />);
  await screen.findByRole("row", { name: /local-model/ });
  await userEvent.click(screen.getByRole("tab", { name: "Limits" }));
};

it("moves between the Usage and Limits tabs with the arrow keys and reads a meter as what is used", async () => {
  vi.spyOn(api, "limits").mockResolvedValue(limitsOf([{ engine: "codex", checked_at: "", limits: {}, plan: null, windows: [codexWindow()] }]));
  render(<UsagePage />);
  await screen.findByRole("row", { name: /local-model/ });
  const usage = screen.getByRole("tab", { name: "Usage" });
  const limits = screen.getByRole("tab", { name: "Limits" });
  expect(usage).toHaveAttribute("aria-selected", "true");
  expect(screen.getByRole("tabpanel")).toHaveAttribute("aria-labelledby", usage.id);
  usage.focus();
  await userEvent.keyboard("{ArrowRight}");
  expect(limits).toHaveAttribute("aria-selected", "true");
  expect(limits).toHaveFocus();
  expect(await screen.findByRole("meter", { name: "5-hour window" })).toHaveAttribute("aria-valuetext", "85% used");
  await userEvent.keyboard("{Home}");
  expect(usage).toHaveAttribute("aria-selected", "true");
});

it("reads Not reported for Claude Code even when its driver sends a window, with no bar and no figure", async () => {
  vi.spyOn(api, "limits").mockResolvedValue(
    limitsOf([{ engine: "claude", checked_at: "", limits: {}, plan: "Personal", windows: [codexWindow({ used_percent: 41 })] }]),
  );
  await openLimits();
  expect(await screen.findByRole("heading", { name: "claude \u00b7 Personal" })).toBeVisible();
  expect(screen.getByText("Not reported")).toBeVisible();
  expect(screen.getByText(/claude doesn\u2019t report one\. Usage and cost are on the Usage tab\./)).toBeVisible();
  expect(screen.queryByRole("meter")).toBeNull();
  expect(screen.queryByText(/% used/)).toBeNull();
});

it("shows a Codex window with its percentage and reset time, neutral at the cap and never amber", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  try {
    const soon = new Date(Date.now() + 38 * 60_000);
    vi.spyOn(api, "limits").mockResolvedValue(
      limitsOf([
        { engine: "claude", checked_at: "", limits: {}, plan: null, windows: [] },
        {
          engine: "codex", checked_at: "", limits: {}, plan: null,
          windows: [
            codexWindow({ used_percent: 93, resets_at: soon.toISOString() }),
            { id: "secondary", label: "7 days", used_percent: 100, window_minutes: 10080, resets_at: new Date(Date.now() + 3 * 86_400_000).toISOString() },
          ],
        },
      ]),
    );
    render(<UsagePage />);
    await screen.findByRole("row", { name: /local-model/ });
    await userEvent.setup({ advanceTimers: vi.advanceTimersByTime }).click(screen.getByRole("tab", { name: "Limits" }));
    const five = await screen.findByRole("group", { name: "5-hour window" });
    expect(five).toHaveTextContent("93% used");
    expect(five).toHaveTextContent("Resets in 38 min");
    expect(five).toHaveTextContent("Near the limit");
    const week = screen.getByRole("group", { name: "Weekly limit" });
    expect(week).toHaveTextContent("100% used");
    expect(week).toHaveTextContent("Limit reached");
    expect(week).toHaveTextContent(/Resets (Sun|Mon|Tue|Wed|Thu|Fri|Sat) \d{1,2} [A-Z][a-z]{2}/);
    // Only the one Not reported row is Claude Code's.
    expect(screen.getAllByText("Not reported")).toHaveLength(1);
    const css = (await import("node:fs")).readFileSync("src/features/usage/usage.css", "utf8");
    expect(css.slice(css.indexOf("/* Limits: a group per engine"), css.indexOf("@media (max-width: 899px)"))).not.toMatch(/warn|amber/);
  } finally {
    vi.useRealTimers();
  }
});

it("sends a signed-out Codex to Providers and checks again", async () => {
  const read = vi.spyOn(api, "limits").mockResolvedValue(limitsOf([], [{ engine: "codex", reason: "Not signed in." }]));
  useWorkspace.setState({ engines: { data: [engine("claude", "Claude Code"), engine("codex", "Codex", { auth: "logged_out" })], error: null, loading: false } });
  await openLimits();
  expect(await screen.findByText("Codex is signed out. Sign in with the CLI, then check again.")).toBeVisible();
  expect(screen.getByRole("link", { name: "Open Providers" })).toHaveAttribute("href", "#/settings/providers");
  expect(screen.queryByText(/Not signed in/)).toBeNull();
  vi.spyOn(api, "limits").mockClear();
  await userEvent.click(screen.getByRole("button", { name: "Check again" }));
  await waitFor(() => expect(read).toHaveBeenCalledWith(true));
});

it("says a Codex read failed, keeps the last figure dimmed with its time, and lists other providers as Not reported", async () => {
  forgetLastLimits();
  const read = vi.spyOn(api, "limits").mockResolvedValueOnce(
    limitsOf([{ engine: "codex", checked_at: "", limits: {}, plan: null, windows: [codexWindow({ used_percent: 62 })] }]),
  );
  useWorkspace.setState({ engines: { data: [engine("codex", "Codex"), engine("opencode", "OpenCode Go")], error: null, loading: false } });
  await openLimits();
  await screen.findByRole("meter", { name: "5-hour window" });
  read.mockResolvedValue(limitsOf([], [{ engine: "codex", reason: "boom" }]));
  await userEvent.click(screen.getByRole("button", { name: "Refresh limits" }));
  expect(await screen.findByText(/Couldn\u2019t read the limit\. Last read \d\d:\d\d\./)).toBeVisible();
  expect(screen.getByRole("button", { name: "Try again" })).toBeVisible();
  expect(screen.queryByText(/boom/)).toBeNull();
  const intro = screen.getByText(/Orbyn reads a limit only through the engine\u2019s own CLI/);
  expect(intro).toBeVisible();
  expect(screen.getByText("OpenCode Go")).toBeVisible();
  expect(screen.getByText("No supported route yet")).toBeVisible();
  expect(screen.getByText("Not reported")).toBeVisible();
});

it("keeps Claude Code's row while Codex is asked, with a placeholder bar and no figure", async () => {
  vi.spyOn(api, "limits").mockReturnValue(new Promise(() => {}));
  useWorkspace.setState({ engines: { data: [engine("claude", "Claude Code"), engine("codex", "Codex")], error: null, loading: false } });
  render(<UsagePage />);
  await screen.findByRole("row", { name: /local-model/ });
  vi.useFakeTimers({ shouldAdvanceTime: true });
  try {
    await userEvent.setup({ advanceTimers: vi.advanceTimersByTime }).click(screen.getByRole("tab", { name: "Limits" }));
    await act(async () => void (await vi.advanceTimersByTimeAsync(500)));
    expect(screen.getByText("Asking Codex\u2026")).toBeVisible();
    expect(screen.getByText("Not reported")).toBeVisible();
    expect(screen.queryByText(/% used/)).toBeNull();
  } finally {
    vi.useRealTimers();
  }
});

it("says nothing reports limits when no engine is installed", async () => {
  vi.spyOn(api, "limits").mockResolvedValue(limitsOf([]));
  await openLimits();
  expect(await screen.findByText("No engine reports limits.")).toBeVisible();
});

it("stops loading Limits after the wait and offers to try again", async () => {
  vi.spyOn(api, "limits").mockReturnValue(new Promise(() => {}));
  render(<UsagePage />);
  await screen.findByRole("row", { name: /local-model/ });
  vi.useFakeTimers({ shouldAdvanceTime: true });
  try {
    await userEvent.setup({ advanceTimers: vi.advanceTimersByTime }).click(screen.getByRole("tab", { name: "Limits" }));
    expect(screen.queryByText(/didn\u2019t answer in time/)).toBeNull();
    await act(async () => void (await vi.advanceTimersByTimeAsync(LIMITS_WAIT_MS + 50)));
    expect(screen.getByText(/Limits didn\u2019t answer in time\./)).toBeVisible();
    expect(screen.getByRole("button", { name: "Try again" })).toBeVisible();
  } finally {
    vi.useRealTimers();
  }
});

it("sets Limits 16px below the tabs, with the group label first", async () => {
  vi.spyOn(api, "limits").mockResolvedValue(limitsOf([{ engine: "claude", checked_at: "", limits: {}, plan: null, windows: [] }]));
  const { container } = render(<UsagePage />);
  await screen.findByRole("row", { name: /local-model/ });
  await userEvent.click(screen.getByRole("tab", { name: "Limits" }));
  await screen.findByRole("heading", { name: "claude" });
  expect(container.querySelector(".usage-body[data-limits]")).not.toBeNull();
  const css = (await import("node:fs")).readFileSync("src/features/usage/usage.css", "utf8");
  expect(css).toMatch(/\.limit-group h2 \{\s*padding: 16px 0 4px/);
});

it("says the history is being read, once: the first visit only, after the wait", async () => {
  localStorage.removeItem("orbyn.usage.seen.v1");
  const slow = deferred<UsageReport>();
  vi.mocked(api.usage).mockReturnValue(slow.promise);
  vi.useFakeTimers({ shouldAdvanceTime: true });
  try {
    const first = render(<UsagePage />);
    expect(screen.queryByText("Reading your history…")).toBeNull();
    await act(async () => void (await vi.advanceTimersByTimeAsync(500)));
    expect(screen.getByText("Reading your history…")).toBeVisible();
    expect(screen.getByText("This happens once.")).toBeVisible();
    await act(async () => slow.resolve(report(5)));
    await waitFor(() => expect(localStorage.getItem("orbyn.usage.seen.v1")).toBe("true"));
    first.unmount();
    // A later visit uses the normal skeleton, not the sentence.
    const again = deferred<UsageReport>();
    vi.mocked(api.usage).mockReturnValue(again.promise);
    render(<UsagePage />);
    await act(async () => void (await vi.advanceTimersByTimeAsync(500)));
    expect(screen.queryByText("Reading your history…")).toBeNull();
  } finally {
    vi.useRealTimers();
  }
});

it("shows Checking your limits… after the wait while Limits loads, and the skeleton before", async () => {
  vi.spyOn(api, "limits").mockReturnValue(new Promise(() => {}));
  render(<UsagePage />);
  await screen.findByRole("row", { name: /local-model/ });
  vi.useFakeTimers({ shouldAdvanceTime: true });
  try {
    await userEvent.setup({ advanceTimers: vi.advanceTimersByTime }).click(screen.getByRole("tab", { name: "Limits" }));
    expect(screen.queryByText("Checking your limits…")).toBeNull();
    await act(async () => void (await vi.advanceTimersByTimeAsync(500)));
    expect(screen.getByText("Checking your limits…")).toBeVisible();
  } finally {
    vi.useRealTimers();
  }
});
