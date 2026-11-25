import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { projects } from "../../../mock/fixtures";
import { useWorkspace } from "../../product/state/workspace";
import { POLL_MS, SLOW_POLL_EVERY } from "../pollSchedule";
import { shouldPoll, startShellPolling } from "./polling";

/** Every request the poll sent, as `METHOD path`. */
const sent: string[] = [];
const count = (path: string) => sent.filter((call) => call === `GET ${path}`).length;
const total = () => sent.length;

/**
 * A service that answers every read the poll makes. One project is registered, so a poll that
 * asked each project for its threads would show up in `sent`.
 */
const answerEveryRead = () => {
  vi.stubGlobal("fetch", vi.fn(async (input: string, init?: RequestInit) => {
    const path = String(input);
    sent.push(`${init?.method ?? "GET"} ${path}`);
    const body = path === "/api/projects"
      ? [projects[0]]
      : path === "/api/threads" || path.startsWith("/api/approvals") || path === "/api/updates"
        ? []
        : {};
    return new Response(JSON.stringify(body), { status: 200 });
  }));
};

/** The browser's own signal: a tab that is hidden, and one that is shown again. */
const setHidden = (hidden: boolean) => {
  Object.defineProperty(document, "hidden", { configurable: true, get: () => hidden });
  Object.defineProperty(document, "visibilityState", { configurable: true, get: () => (hidden ? "hidden" : "visible") });
  document.dispatchEvent(new Event("visibilitychange"));
};

const IDLE_MS = 120_000;
const TICKS = IDLE_MS / POLL_MS;

/** Runs the poll over a window of the app's own clock; no test here waits on real time. */
const pollFor = (ms: number) => vi.advanceTimersByTimeAsync(ms);

/** Lets the reads a decision started actually reach the service, without moving the clock. */
const flush = () => vi.advanceTimersByTimeAsync(0);

describe("the shell poll", () => {
  let stop: () => void;

  beforeEach(() => {
    sent.length = 0;
    answerEveryRead();
    setHidden(false);
    vi.useFakeTimers();
    stop = startShellPolling();
  });

  afterEach(() => {
    stop();
    vi.useRealTimers();
  });

  it("reads the three lists every tick and one thread list for the whole app", async () => {
    await pollFor(IDLE_MS);
    // Projects, the one thread list, and approvals: the idle app's own traffic.
    expect(total()).toBe(TICKS * 3);
    expect(count("/api/projects")).toBe(TICKS);
    expect(count("/api/threads")).toBe(TICKS);
    expect(count("/api/approvals?state=pending")).toBe(TICKS);
    // The filtered list is a slice of the unfiltered one, so a poll never asks for it.
    expect(sent.filter((call) => call.includes("project_id="))).toEqual([]);
  });

  it("leaves the Git-backed status alone until its own slower tick", async () => {
    await pollFor(IDLE_MS);
    expect(count("/api/workspace")).toBe(0);
    expect(count("/api/updates")).toBe(0);
    await pollFor((SLOW_POLL_EVERY - TICKS) * POLL_MS);
    expect(count("/api/workspace")).toBe(1);
    expect(count("/api/updates")).toBe(1);
  });

  it("sends nothing at all while the window is hidden", async () => {
    setHidden(true);
    await pollFor(IDLE_MS);
    expect(sent).toEqual([]);
  });

  it("keeps reading a hidden window that is running work, or its alerts would never fire", async () => {
    useWorkspace.setState({
      threads: { t: { id: "t", project_id: "p-app", active_run: { id: "r" } } as never },
    });
    setHidden(true);
    await pollFor(IDLE_MS);
    expect(count("/api/projects")).toBe(TICKS);
  });

  it("reads once at once when the window is shown again, and stops reading when it hides", async () => {
    setHidden(true);
    await pollFor(IDLE_MS);
    expect(sent).toEqual([]);
    const lists = [count("/api/projects"), count("/api/threads"), count("/api/approvals?state=pending")];
    setHidden(false);
    await flush();
    // Shown again: exactly one read of what it could not see while hidden.
    expect([count("/api/projects"), count("/api/threads"), count("/api/approvals?state=pending")])
      .toEqual(lists.map((n) => n + 1));
    await pollFor(POLL_MS);
    // The ordinary cadence carries on from there: the ticks the hidden window skipped sent
    // nothing but still counted, so this is the second read, not the thirty-second.
    expect(count("/api/projects")).toBe(2);
    setHidden(true);
    const afterHiding = total();
    await pollFor(IDLE_MS);
    expect(total()).toBe(afterHiding);
  });
});

describe("one poll decision", () => {
  it("reads a shown window and a hidden one that is running work", () => {
    expect(shouldPoll(false, false)).toBe(true);
    expect(shouldPoll(false, true)).toBe(true);
    expect(shouldPoll(true, true)).toBe(true);
    expect(shouldPoll(true, false)).toBe(false);
  });
});
