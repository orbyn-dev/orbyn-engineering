import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { request } from "./client";
import { resetSharedReadsForTests } from "./singleFlight";

interface Pending {
  path: string;
  signal: AbortSignal | undefined;
  answer: (body: unknown) => void;
}

/** A fetch whose answers the test gives by hand, so the requests in flight can be counted. */
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
  return { pending, fetchMock };
}

// Real wait: one macrotask turn lets already-resolved promises settle; no clock is involved.
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

const shared = <T>(url: string, signal?: AbortSignal) =>
  request<T>("GET", url, undefined, signal, { share: true });

beforeEach(resetSharedReadsForTests);
afterEach(() => vi.unstubAllGlobals());

describe("opt-in single-flight reads", () => {
  it("sends one request for reads made together and gives every caller the data", async () => {
    const { pending, fetchMock } = holdFetch();
    const calls = [1, 2, 3, 4].map(() => shared<{ rows: number[] }>("/api/threads"));
    await tick();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    pending[0]?.answer({ rows: [1] });
    const answers = await Promise.all(calls);
    for (const answer of answers) expect(answer).toEqual({ rows: [1] });
    // Two separate requests would have answered with two objects; the callers do not share one.
    expect(answers[0]).not.toBe(answers[1]);
    expect(answers[0]).not.toBe(answers[3]);
  });

  it("keeps reads that did not opt in, different URLs and different bodies apart", async () => {
    const { pending, fetchMock } = holdFetch();
    const calls = [
      // The same read twice without opting in stays two requests.
      request("GET", "/api/info"),
      request("GET", "/api/info"),
      // The same read twice with sharing is one.
      shared("/api/info"),
      shared("/api/info"),
      // Reads of different URLs are different reads.
      shared("/api/threads?project_id=a"),
      shared("/api/threads?project_id=b"),
      // A read-shaped POST shares only with the same body.
      request("POST", "/api/settings/preferences/query", { keys: ["a"] }, undefined, { share: true }),
      request("POST", "/api/settings/preferences/query", { keys: ["b"] }, undefined, { share: true }),
    ];
    await tick();
    const urls = fetchMock.mock.calls.map(([url]) => String(url));
    const count = (url: string) => urls.filter((entry) => entry === url).length;
    expect(count("/api/info")).toBe(3);
    expect(count("/api/threads?project_id=a")).toBe(1);
    expect(count("/api/threads?project_id=b")).toBe(1);
    expect(count("/api/settings/preferences/query")).toBe(2);
    expect(urls).toHaveLength(7);
    for (const entry of pending) entry.answer({});
    await Promise.all(calls);
  });

  it("delivers one failure to every caller and starts the next read anew", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response("no", { status: 500 })));
    const first = [shared("/api/threads"), shared("/api/threads")];
    const failures = await Promise.all(first.map((call) => call.catch((error: unknown) => error)));
    for (const failure of failures) expect(failure).toMatchObject({ code: "http_500", status: 500 });
    await expect(shared("/api/threads")).rejects.toMatchObject({ code: "http_500" });
    expect(vi.mocked(fetch)).toHaveBeenCalledTimes(2);
  });

  it("never joins a write, and a read made after a write starts its own request", async () => {
    const { pending, fetchMock } = holdFetch();
    const before = shared("/api/threads");
    await tick();
    const write = request("POST", "/api/threads/order", { ids: [] });
    const after = shared("/api/threads");
    await tick();
    expect(fetchMock).toHaveBeenCalledTimes(3);
    for (const entry of pending) entry.answer([]);
    await Promise.all([before, write, after]);
  });

  it("never joins a read when a fresh answer is asked for", async () => {
    const { pending, fetchMock } = holdFetch();
    const held = shared("/api/info");
    const fresh = request("GET", "/api/info", undefined, undefined, { share: true, fresh: true });
    await tick();
    expect(fetchMock).toHaveBeenCalledTimes(2);
    for (const entry of pending) entry.answer({});
    await Promise.all([held, fresh]);
  });

  it("keeps the request while one caller still waits, and abandons it only when all have gone", async () => {
    const { pending, fetchMock } = holdFetch();
    const one = new AbortController();
    const two = new AbortController();
    const a = shared("/api/threads", one.signal);
    const b = shared("/api/threads", two.signal);
    await tick();
    one.abort();
    await expect(a).rejects.toMatchObject({ name: "AbortError" });
    expect(pending[0]?.signal?.aborted).toBe(false);
    two.abort();
    await expect(b).rejects.toMatchObject({ name: "AbortError" });
    expect(pending[0]?.signal?.aborted).toBe(true);
    // A later identical read starts a new request instead of joining the abandoned one.
    const c = shared<unknown[]>("/api/threads");
    await tick();
    expect(fetchMock).toHaveBeenCalledTimes(2);
    pending[1]?.answer([]);
    await expect(c).resolves.toEqual([]);
  });
});
