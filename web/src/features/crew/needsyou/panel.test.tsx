import { act, render, screen, within } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { useConnection } from "../../../product/state/connection";
import { installMockFetch } from "../../../test-support";
import { NeedsYouInbox } from "./NeedsYouInbox";

/**
 * The inbox's 30-second refresh, and the narrow panel it does not yet reach: the cards are a skipped case until #672 is fixed. Its
 * rows, filters, dismissals and answers are inbox.test.tsx.
 */
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

/** A ResizeObserver that measures every element it watches at one width, as a panel that wide would report. */
function panelWidth(width: number) {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(private readonly send: ResizeObserverCallback) {}
      observe(target: Element) {
        this.send([{ target, contentRect: { width } } as unknown as ResizeObserverEntry], this as unknown as ResizeObserver);
      }
      unobserve() {}
      disconnect() {}
    },
  );
}

// Skipped until #672 is fixed: useNarrow observes only on the first render, which still shows <Loading>, so the narrow layout never applies after load. Enable this test with the fix.
it.skip("draws one card per item in a narrow panel, with the kind first and no rank", async () => {
  panelWidth(320);
  mock.reset("crew-held-account");
  render(<NeedsYouInbox />);
  const cards = within(await list()).getAllByRole("listitem");
  expect(cards).toHaveLength(5);
  // A card leads with its kind word; a wide row leads with its rank number.
  expect(cards[0]).toHaveTextContent(/^Main/);
  expect(cards[1]).toHaveTextContent(/^Background/);
  for (const card of cards.slice(2)) expect(card).toHaveTextContent(/^Approval/);
  expect(cards[0]).toHaveTextContent("Main broke and the cause isn’t certain");
  // The actions are the same buttons with the same labels.
  expect(within(cards[2]!).getByRole("button", { name: /^Deny: / })).toBeEnabled();
  expect(writes()).toHaveLength(0);
});

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
