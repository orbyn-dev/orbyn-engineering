import { afterEach, expect, it, vi } from "vitest";
import { earlierApi, setEarlierApi, type EarlierPage } from "./earlierContract";

afterEach(() => {
  setEarlierApi(null);
  vi.unstubAllGlobals();
});

const PAGE: EarlierPage = {
  source: "Claude Code",
  total: 312,
  imported_at: "2026-11-21T10:00:00Z",
  next_before: "c1",
  messages: [{ role: "user", text: "The logout page loops." }],
};

const answer = (status: number, body: unknown) =>
  vi.fn(async (_url: string, _init?: RequestInit) => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } }));

it("reads one page from the encoded thread path and reads the cursor as an encoded query", async () => {
  const fetching = answer(200, PAGE);
  vi.stubGlobal("fetch", fetching);
  await expect(earlierApi().read("t 1", { before: "a/b?c" }, new AbortController().signal)).resolves.toEqual(PAGE);
  const [url, init] = fetching.mock.calls[0]!;
  expect(String(url)).toBe("/api/threads/t%201/earlier?before=a%2Fb%3Fc");
  expect(init?.method).toBe("GET");
  expect(init?.body).toBeUndefined();
});

it("sends no cursor query for the last page", async () => {
  const fetching = answer(200, PAGE);
  vi.stubGlobal("fetch", fetching);
  await earlierApi().read("t1", {}, new AbortController().signal);
  expect(String(fetching.mock.calls[0]![0])).toBe("/api/threads/t1/earlier");
});

it("reads a 404 as a thread with no earlier history, whether or not the code says not_imported", async () => {
  vi.stubGlobal("fetch", answer(404, { code: "not_imported", message: "Not imported." }));
  await expect(earlierApi().read("t1", {}, new AbortController().signal)).resolves.toBeNull();
  vi.stubGlobal("fetch", vi.fn(async () => new Response("", { status: 404 })));
  await expect(earlierApi().read("t1", {}, new AbortController().signal)).resolves.toBeNull();
});

it("lets a history that cannot be read through with its code", async () => {
  vi.stubGlobal("fetch", answer(422, { code: "history_unreadable", message: "The session file has moved." }));
  await expect(earlierApi().read("t1", {}, new AbortController().signal)).rejects.toMatchObject({
    code: "history_unreadable",
    status: 422,
  });
});

it("lets a bad cursor through with its code, for the screen to read as a changed history", async () => {
  vi.stubGlobal("fetch", answer(422, { code: "bad_cursor", message: "The cursor is stale." }));
  await expect(earlierApi().read("t1", { before: "c1" }, new AbortController().signal)).rejects.toMatchObject({
    code: "bad_cursor",
    status: 422,
  });
});
