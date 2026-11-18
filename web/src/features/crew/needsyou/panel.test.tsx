import { act, render, screen } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { useConnection } from "../../../product/state/connection";
import { installMockFetch } from "../../../test-support";
import { NeedsYouInbox } from "./NeedsYouInbox";

/** The inbox's 30-second refresh. Its rows, filters, dismissals and answers are inbox.test.tsx. */
let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  useConnection.setState({ remote: false });
  window.location.hash = "";
  window.localStorage.clear();
});

const urls = () => vi.mocked(fetch).mock.calls.map(([url]) => String(url));
const reads = (path: string) => urls().filter((url) => url.includes(path)).length;
const writes = () => vi.mocked(fetch).mock.calls.filter(([, init]) => init?.method && init.method !== "GET");
const list = () => screen.findByRole("list", { name: "Needs you" });

it("reads the inbox again every 30 seconds while the window is visible, and not while it is hidden", async () => {
  mock.reset("crew");
  const first = render(<NeedsYouInbox />);
  await list();
  // The timer is set when the inbox mounts, so a fresh inbox under the fake clock is the one that is read again.
  first.unmount();
  vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
  try {
    render(<NeedsYouInbox />);
    await list();
    const stops = reads("/api/crew/stops");
    const review = reads("/api/crew/autopilot/review");
    const background = reads("/api/crew/background?");
    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_000);
    });
    expect(reads("/api/crew/stops")).toBe(stops + 1);
    expect(reads("/api/crew/autopilot/review")).toBe(review + 1);
    expect(reads("/api/crew/background?")).toBe(background + 1);
    Object.defineProperty(document, "visibilityState", { configurable: true, get: () => "hidden" });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(90_000);
    });
    expect(reads("/api/crew/stops")).toBe(stops + 1);
    expect(reads("/api/crew/autopilot/review")).toBe(review + 1);
    // Back in view, the same timer reads again: the hidden stretch skipped its ticks, it did not stop the timer.
    Object.defineProperty(document, "visibilityState", { configurable: true, get: () => "visible" });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_000);
    });
    expect(reads("/api/crew/stops")).toBe(stops + 2);
    expect(reads("/api/crew/autopilot/review")).toBe(review + 2);
  } finally {
    Object.defineProperty(document, "visibilityState", { configurable: true, get: () => "visible" });
    vi.useRealTimers();
  }
  expect(writes()).toHaveLength(0);
});
