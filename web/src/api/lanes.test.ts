import { afterEach, describe, expect, it, vi } from "vitest";
import { request } from "./client";
import { SLOW_LANE_CAP, laneFor } from "./lanes";

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

afterEach(() => vi.unstubAllGlobals());

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
      "/api/workspace",
      "/api/updates",
      "/api/usage?days=30",
      "/api/limits",
      "/api/forges",
      "/api/projects/p1/pulls/12/checks",
    ]) {
      expect(laneFor("GET", path), path).toBe("slow");
    }
  });

  it("keeps the fast reads and every write interactive", () => {
    expect(laneFor("GET", "/api/projects")).toBe("interactive");
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
});

describe("shared reads", () => {
  it("shares one fetch between identical reads and gives each caller its own copy", async () => {
    const { pending, fetchMock } = holdFetch();
    const a = request<{ rows: number[] }>("GET", "/api/threads?project_id=p1");
    const b = request<{ rows: number[] }>("GET", "/api/threads?project_id=p1");
    await tick();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    pending[0]?.answer({ rows: [1] });
    const [one, two] = await Promise.all([a, b]);
    expect(one).toEqual({ rows: [1] });
    expect(one).not.toBe(two);
    one.rows.push(2);
    expect(two.rows).toEqual([1]);
  });

  it("does not share reads that differ in query, or a write", async () => {
    const { pending, fetchMock } = holdFetch();
    const calls = [
      request("GET", "/api/threads?project_id=p1"),
      request("GET", "/api/threads?project_id=p2"),
      request("POST", "/api/queue", {}),
      request("POST", "/api/queue", {}),
    ];
    await tick();
    expect(fetchMock).toHaveBeenCalledTimes(4);
    for (const entry of pending) entry.answer({});
    await Promise.all(calls);
  });

  it("aborts the shared request only when every caller has aborted", async () => {
    const { pending, fetchMock } = holdFetch();
    const one = new AbortController();
    const two = new AbortController();
    const a = request("GET", "/api/projects", undefined, one.signal);
    const b = request("GET", "/api/projects", undefined, two.signal);
    await tick();
    one.abort();
    await expect(a).rejects.toMatchObject({ name: "AbortError" });
    expect(pending[0]?.signal?.aborted).toBe(false);
    two.abort();
    await expect(b).rejects.toMatchObject({ name: "AbortError" });
    expect(pending[0]?.signal?.aborted).toBe(true);

    // A later identical read starts a new request instead of joining the abandoned one.
    const c = request("GET", "/api/projects");
    await tick();
    expect(fetchMock).toHaveBeenCalledTimes(2);
    pending[1]?.answer([]);
    await expect(c).resolves.toEqual([]);
  });

  it("lets one caller finish when another aborts", async () => {
    const { pending } = holdFetch();
    const gone = new AbortController();
    const a = request("GET", "/api/projects", undefined, gone.signal);
    const b = request<string[]>("GET", "/api/projects");
    await tick();
    gone.abort();
    await expect(a).rejects.toMatchObject({ name: "AbortError" });
    pending[0]?.answer(["x"]);
    await expect(b).resolves.toEqual(["x"]);
  });
});
