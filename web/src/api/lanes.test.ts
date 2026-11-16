import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NETWORK_ERROR, request } from "./client";
import { LANE_TIMEOUT_MS, asBackground, SLOW_LANE_CAP, laneFor, resetLanesForTests } from "./lanes";

interface Pending {
  path: string;
  signal: AbortSignal | undefined;
  answer: (body: unknown) => void;
}

/** A fetch whose answers the test gives by hand, so what is in flight can be counted. */
function holdFetch() {
  const pending: Pending[] = [];
  const fetchMock = vi.fn((url: string, init?: RequestInit) => {
    const signal = init?.signal ?? undefined;
    return new Promise<Response>((resolve, reject) => {
      const path = url.replace(/^https?:\/\/[^/]+/, "");
      signal?.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError")));
      pending.push({ path, signal, answer: (body) => resolve(new Response(JSON.stringify(body), { status: 200 })) });
    });
  });
  vi.stubGlobal("fetch", fetchMock);
  return { pending, fetchMock, inFlight: () => pending.filter((entry) => !entry.signal?.aborted) };
}

const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

/** Answers everything that starts, including what only starts once a slot frees up. */
async function finishAll(pending: Pending[]) {
  for (let round = 0; round < 10; round += 1) {
    const before = pending.length;
    for (const entry of [...pending]) entry.answer({});
    await tick();
    if (pending.length === before) return;
  }
}

beforeEach(resetLanesForTests);
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("laneFor", () => {
  it("sends the slow reads to the slow lane", () => {
    for (const path of [
      "/api/pulls?state=open&q=",
      "/api/engines/claude/models",
      "/api/engines/claude/models?refresh=true",
      "/api/threads/t1/tools",
      "/api/threads/t1/commands",
      "/api/threads/t1/limits",
      "/api/health/dependencies",
      "/api/usage?days=30",
      "/api/limits",
      "/api/projects/p1/pulls/12/checks",
    ]) {
      expect(laneFor("GET", path), path).toBe("slow");
    }
  });

  it("keeps the fast reads and every write interactive", () => {
    expect(laneFor("GET", "/api/projects")).toBe("interactive");
    expect(laneFor("GET", "/api/forges")).toBe("interactive");
    expect(laneFor("GET", "/api/workspace")).toBe("interactive");
    expect(laneFor("GET", "/api/updates")).toBe("interactive");
    expect(laneFor("GET", "/api/threads?project_id=p1")).toBe("interactive");
    expect(laneFor("GET", "/api/threads/t1")).toBe("interactive");
    expect(laneFor("POST", "/api/pulls/details")).toBe("interactive");
    expect(laneFor("POST", "/api/usage/refresh?days=30")).toBe("interactive");
    expect(laneFor("PATCH", "/api/threads/t1")).toBe("interactive");
    expect(laneFor("DELETE", "/api/projects/p1")).toBe("interactive");
  });
});

describe("request lanes", () => {
  it("does not delay a fast request behind four slow ones", async () => {
    const { pending, inFlight } = holdFetch();
    const slow = [
      request("GET", "/api/pulls?state=open&q="),
      request("GET", "/api/engines/a/models"),
      request("GET", "/api/threads/t1/tools"),
      request("GET", "/api/threads/t1/limits"),
    ];
    const fast = request<string>("GET", "/api/projects");
    await tick();
    expect(inFlight().filter((entry) => entry.path !== "/api/projects")).toHaveLength(SLOW_LANE_CAP);
    expect(pending.some((entry) => entry.path === "/api/projects")).toBe(true);
    pending.find((entry) => entry.path === "/api/projects")?.answer("fast");
    await expect(fast).resolves.toBe("fast");

    // The queued ones start in arrival order as the running ones finish.
    pending.find((entry) => entry.path.startsWith("/api/pulls"))?.answer(1);
    await tick();
    expect(pending.map((entry) => entry.path)).toContain("/api/threads/t1/tools");
    expect(pending.map((entry) => entry.path)).not.toContain("/api/threads/t1/limits");
    await finishAll(pending);
    await Promise.all(slow);
  });

  it("never queues a write behind slow reads", async () => {
    const { pending } = holdFetch();
    const reads = ["/api/pulls", "/api/usage?days=7", "/api/limits"].map((path) => request("GET", path));
    const write = request("POST", "/api/threads/t1/runs", { prompt: "hi" });
    await tick();
    expect(pending.some((entry) => entry.path === "/api/threads/t1/runs")).toBe(true);
    await finishAll(pending);
    await Promise.all([...reads, write]);
  });

  it("honours an abort while a slow read is still queued", async () => {
    const { pending } = holdFetch();
    const first = [request("GET", "/api/pulls"), request("GET", "/api/usage?days=7")];
    const abort = new AbortController();
    const queued = request("GET", "/api/limits", undefined, abort.signal);
    await tick();
    abort.abort();
    await expect(queued).rejects.toMatchObject({ name: "AbortError" });
    expect(pending.map((entry) => entry.path)).not.toContain("/api/limits");
    await finishAll(pending);
    await Promise.all(first);
    expect(pending.map((entry) => entry.path)).not.toContain("/api/limits");
  });

  it("frees the slow slot when a read fails", async () => {
    const calls: string[] = [];
    vi.stubGlobal("fetch", vi.fn(async (url: string) => {
      calls.push(url);
      return new Response("no", { status: 500 });
    }));
    const reads = ["/api/pulls", "/api/usage?days=7", "/api/limits", "/api/updates"].map((path) =>
      request("GET", path).catch(() => "failed"),
    );
    await expect(Promise.all(reads)).resolves.toEqual(["failed", "failed", "failed", "failed"]);
    expect(calls).toHaveLength(4);
  });

  it("lets a read the person starts jump ahead of queued background reads", async () => {
    vi.useFakeTimers();
    const { pending } = holdFetch();
    const running = [request("GET", "/api/pulls?state=open&q="), request("GET", "/api/usage?days=7")];
    const background = ["/api/limits", "/api/engines/a/models", "/api/threads/t1/tools"].map((path) =>
      asBackground(() => request("GET", path)),
    );
    const person = request("GET", "/api/threads/t1/commands");
    await vi.advanceTimersByTimeAsync(1);
    expect(pending.map((entry) => entry.path)).toEqual(["/api/pulls?state=open&q=", "/api/usage?days=7"]);
    pending[0]?.answer({});
    await vi.advanceTimersByTimeAsync(1);
    // The person's read took the slot, though three background reads were queued first.
    expect(pending.map((entry) => entry.path)).toContain("/api/threads/t1/commands");
    expect(pending.map((entry) => entry.path)).not.toContain("/api/limits");
    // The cap holds, and the background reads still run, in arrival order, once the person's is done.
    for (let round = 0; round < 8; round += 1) {
      for (const entry of [...pending]) entry.answer({});
      await vi.advanceTimersByTimeAsync(1);
    }
    expect(pending.map((entry) => entry.path).slice(-3)).toEqual(["/api/limits", "/api/engines/a/models", "/api/threads/t1/tools"]);
    await Promise.all([...running, ...background, person]);
  });

  it("lets a foreground read bypass a full slow lane", async () => {
    const { pending, fetchMock } = holdFetch();
    const slow = [request("GET", "/api/pulls?state=open&q="), request("GET", "/api/usage?days=7"), request("GET", "/api/limits")];
    const a = request("GET", "/api/projects/p1/pulls/3", undefined, undefined, { priority: "foreground" });
    await tick();
    expect(fetchMock).toHaveBeenCalledTimes(3);
    expect(pending.some((entry) => entry.path === "/api/projects/p1/pulls/3")).toBe(true);
    await finishAll(pending);
    await Promise.all([...slow, a]);
  });
});

describe("the slow lane timeout", () => {
  it("abandons a hung read as a network failure and frees its slot", async () => {
    vi.useFakeTimers();
    const { pending } = holdFetch();
    const hung = [request("GET", "/api/pulls"), request("GET", "/api/usage?days=7")];
    const caught = hung.map((call) => call.catch((error: unknown) => error));
    const waiting = request<string>("GET", "/api/limits");
    await vi.advanceTimersByTimeAsync(1);
    expect(pending.map((entry) => entry.path)).not.toContain("/api/limits");
    await vi.advanceTimersByTimeAsync(LANE_TIMEOUT_MS);
    const errors = await Promise.all(caught);
    for (const error of errors) expect(error).toMatchObject({ code: NETWORK_ERROR });
    expect(pending[0]?.signal?.aborted).toBe(true);
    const next = pending.find((entry) => entry.path === "/api/limits");
    expect(next).toBeDefined();
    next?.answer("ok");
    await expect(waiting).resolves.toBe("ok");
  });
});
