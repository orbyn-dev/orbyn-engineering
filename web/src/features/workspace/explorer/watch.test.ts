import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { reserveStreamConnection } from "../../../api/lanes";
import { dataChanged } from "../../../api/shell";
import type { ExplorerWorktree } from "../../../generated/ExplorerWorktree";
import type { Row } from "./model";
import { followExplorer, watchedWorktrees } from "./watch";

vi.mock("../../../api/lanes", () => ({ reserveStreamConnection: vi.fn() }));
vi.mock("../../../api/shell", () => ({ dataChanged: vi.fn() }));
vi.mock("../../../api/client", () => ({ authHeaders: () => ({}) }));
vi.mock("../../../api/origin", () => ({ serviceUrl: (path: string) => path }));

let stop: (() => void) | undefined;
const fetcher = vi.fn<typeof fetch>();
let focused = true;
let hidden = false;
beforeEach(() => {
  vi.useFakeTimers();
  focused = true;
  hidden = false;
  vi.mocked(reserveStreamConnection).mockReset().mockResolvedValue(vi.fn());
  vi.spyOn(document, "hasFocus").mockImplementation(() => focused);
  vi.spyOn(document, "hidden", "get").mockImplementation(() => hidden);
  fetcher.mockReset().mockImplementation((_url, options) => new Promise((_resolve, reject) => {
    options?.signal?.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError")));
  }));
  vi.stubGlobal("fetch", fetcher);
});
afterEach(() => {
  stop?.();
  stop = undefined;
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

const pair = '[["p","wt-123"]]';
const signal = (at: number) => {
  const found = fetcher.mock.calls[at]?.[1]?.signal;
  if (!found) throw new Error("Expected a request with an abort signal.");
  return found;
};

it("holds one stream while focused, aborts on blur, and reacquires on focus", async () => {
  stop = followExplorer(pair);
  await vi.advanceTimersByTimeAsync(0);
  expect(fetcher).toHaveBeenCalledTimes(1);
  expect(fetcher.mock.calls[0]?.[0]).toContain(encodeURIComponent(pair));
  window.dispatchEvent(new Event("focus"));
  expect(fetcher).toHaveBeenCalledTimes(1);
  focused = false;
  window.dispatchEvent(new Event("blur"));
  expect(signal(0).aborted).toBe(true);
  await vi.advanceTimersByTimeAsync(10_000);
  expect(fetcher).toHaveBeenCalledTimes(1);
  focused = true;
  window.dispatchEvent(new Event("focus"));
  await vi.advanceTimersByTimeAsync(0);
  expect(fetcher).toHaveBeenCalledTimes(2);
  stop();
  expect(signal(1).aborted).toBe(true);
});

it("does not connect while hidden or unfocused and releases when hidden", async () => {
  focused = false;
  stop = followExplorer(pair);
  expect(fetcher).not.toHaveBeenCalled();
  focused = true;
  hidden = true;
  document.dispatchEvent(new Event("visibilitychange"));
  expect(fetcher).not.toHaveBeenCalled();
  hidden = false;
  document.dispatchEvent(new Event("visibilitychange"));
  await vi.advanceTimersByTimeAsync(0);
  expect(fetcher).toHaveBeenCalledTimes(1);
  hidden = true;
  document.dispatchEvent(new Event("visibilitychange"));
  expect(signal(0).aborted).toBe(true);
});

it("reconnects a failed stream without issuing freshness reads and cancels retry on unmount", async () => {
  fetcher.mockRejectedValue(new Error("offline"));
  stop = followExplorer(pair);
  await vi.advanceTimersByTimeAsync(3000);
  expect(fetcher).toHaveBeenCalledTimes(2);
  expect(fetcher.mock.calls.every(([url]) => String(url).startsWith("/api/explorer/events?"))).toBe(true);
  stop();
  await vi.advanceTimersByTimeAsync(30_000);
  expect(fetcher).toHaveBeenCalledTimes(2);
});

it("does not retry on its own after a 4xx refusal", async () => {
  fetcher.mockResolvedValue(new Response("nope", { status: 400 }));
  stop = followExplorer(pair);
  await vi.advanceTimersByTimeAsync(3000);
  expect(fetcher).toHaveBeenCalledTimes(1);
  await vi.advanceTimersByTimeAsync(120_000);
  expect(fetcher).toHaveBeenCalledTimes(1);
});

it("backs off exponentially after repeated server failures, capped at one minute", async () => {
  fetcher.mockResolvedValue(new Response("down", { status: 503 }));
  stop = followExplorer(pair);
  await vi.advanceTimersByTimeAsync(0);
  expect(fetcher).toHaveBeenCalledTimes(1);
  await vi.advanceTimersByTimeAsync(2999);
  expect(fetcher).toHaveBeenCalledTimes(1);
  await vi.advanceTimersByTimeAsync(1);
  expect(fetcher).toHaveBeenCalledTimes(2);
  await vi.advanceTimersByTimeAsync(5999);
  expect(fetcher).toHaveBeenCalledTimes(2);
  await vi.advanceTimersByTimeAsync(1);
  expect(fetcher).toHaveBeenCalledTimes(3);
  await vi.advanceTimersByTimeAsync(12_000 + 24_000 + 48_000);
  expect(fetcher).toHaveBeenCalledTimes(6);
  await vi.advanceTimersByTimeAsync(59_999);
  expect(fetcher).toHaveBeenCalledTimes(6);
  await vi.advanceTimersByTimeAsync(1);
  expect(fetcher).toHaveBeenCalledTimes(7);
});

it("starts the backoff over only after a ready stream that stayed up for 30 s", async () => {
  const frame = new TextEncoder().encode('data: {"type":"data_changed","what":"explorer:p"}\n\n');
  let close: (() => void) | undefined;
  const live = () => new Response(new ReadableStream({ start(controller) {
    controller.enqueue(frame);
    close = () => controller.close();
  } }));
  fetcher.mockResolvedValueOnce(new Response("down", { status: 503 }));
  fetcher.mockResolvedValueOnce(new Response("down", { status: 503 }));
  fetcher.mockImplementationOnce(async () => live());
  fetcher.mockResolvedValue(new Response("down", { status: 503 }));
  stop = followExplorer(pair);
  await vi.advanceTimersByTimeAsync(3000 + 6000);
  expect(fetcher).toHaveBeenCalledTimes(3);
  await vi.advanceTimersByTimeAsync(30_000);
  close?.();
  await vi.advanceTimersByTimeAsync(2999);
  expect(fetcher).toHaveBeenCalledTimes(3);
  await vi.advanceTimersByTimeAsync(1);
  expect(fetcher).toHaveBeenCalledTimes(4);
});

it("keeps backing off after a stream that said ready and then failed at once", async () => {
  const flash = () => new Response(new ReadableStream({ start(controller) {
    controller.enqueue(new TextEncoder().encode('data: {"type":"data_changed","what":"explorer:p"}\n\n'));
    controller.close();
  } }));
  fetcher.mockImplementation(async () => flash());
  stop = followExplorer(pair);
  await vi.advanceTimersByTimeAsync(0);
  expect(fetcher).toHaveBeenCalledTimes(1);
  await vi.advanceTimersByTimeAsync(3000);
  expect(fetcher).toHaveBeenCalledTimes(2);
  await vi.advanceTimersByTimeAsync(5999);
  expect(fetcher).toHaveBeenCalledTimes(2);
  await vi.advanceTimersByTimeAsync(1);
  expect(fetcher).toHaveBeenCalledTimes(3);
});

it("turns a ready frame into the existing explorer refresh hint", async () => {
  fetcher.mockResolvedValue(new Response(new ReadableStream({ start(controller) {
    controller.enqueue(new TextEncoder().encode('data: {"type":"data_changed","what":"explorer:p"}\n\n'));
  } })));
  stop = followExplorer(pair);
  await vi.advanceTimersByTimeAsync(0);
  expect(dataChanged).toHaveBeenCalledWith("explorer:p");
});

it("a focus retry replaces the pending reconnect instead of starting a second timer chain", async () => {
  fetcher.mockRejectedValue(new Error("offline"));
  stop = followExplorer(pair);
  await vi.advanceTimersByTimeAsync(1000);
  window.dispatchEvent(new Event("focus"));
  await vi.advanceTimersByTimeAsync(2000);
  expect(fetcher).toHaveBeenCalledTimes(2);
  // The focus retry was the second failure, so the one live timer waits 6 s from it.
  await vi.advanceTimersByTimeAsync(3999);
  expect(fetcher).toHaveBeenCalledTimes(2);
  await vi.advanceTimersByTimeAsync(1);
  expect(fetcher).toHaveBeenCalledTimes(3);
});

it("releases its connection reservation on blur and aborts a queued reservation before fetching", async () => {
  const release = vi.fn();
  vi.mocked(reserveStreamConnection).mockResolvedValueOnce(release);
  stop = followExplorer(pair);
  await vi.advanceTimersByTimeAsync(0);
  focused = false;
  window.dispatchEvent(new Event("blur"));
  await vi.advanceTimersByTimeAsync(0);
  expect(release).toHaveBeenCalledTimes(1);
  stop();
  vi.mocked(reserveStreamConnection).mockImplementation((signal) => new Promise((_resolve, reject) => {
    signal.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError")));
  }));
  focused = true;
  stop = followExplorer(pair);
  focused = false;
  window.dispatchEvent(new Event("blur"));
  await vi.advanceTimersByTimeAsync(0);
  expect(fetcher).toHaveBeenCalledTimes(1);
});

it("only watches unfolded present worktrees, deduplicated and capped per window", () => {
  const row = (id: string, open = true, missing = false): Row => ({
    kind: "worktree", key: id, depth: 2, projectId: "p", label: id, open, missing,
    worktree: { id } as ExplorerWorktree,
  });
  expect(watchedWorktrees([row("a"), row("a"), row("closed", false), row("gone", true, true)])).toBe('[["p","a"]]');
  expect(JSON.parse(watchedWorktrees(Array.from({ length: 50 }, (_, i) => row(String(i)))))).toHaveLength(32);
  stop = followExplorer("[]");
  expect(fetcher).not.toHaveBeenCalled();
});
