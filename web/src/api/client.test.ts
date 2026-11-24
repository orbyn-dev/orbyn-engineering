import { describe, expect, it, vi } from "vitest";
import { api, ApiFailure, NETWORK_ERROR, request, setUnauthorizedHandler } from "./client";

const respond = (status: number, body?: unknown) =>
  vi.fn(async () => new Response(body === undefined ? null : JSON.stringify(body), { status }));

describe("request", () => {
  it("sends the bearer token when one is held", async () => {
    sessionStorage.setItem("orbyn.token", "tok");
    const fetchMock = respond(200, {});
    vi.stubGlobal("fetch", fetchMock);
    await request("GET", "/api/info");
    const [, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(init.headers).toMatchObject({ Authorization: "Bearer tok" });
  });

  it("maps an ApiError body to its stable code", async () => {
    vi.stubGlobal("fetch", respond(409, { code: "thread_busy", message: "Busy." }));
    await expect(request("POST", "/x")).rejects.toMatchObject({
      code: "thread_busy",
      message: "Busy.",
      status: 409,
    });
  });

  it("falls back to the status for a non-JSON error", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response("nope", { status: 502 })));
    await expect(request("GET", "/x")).rejects.toMatchObject({ code: "http_502" });
  });

  it("reports an unreachable service as a network failure", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => Promise.reject(new TypeError("failed"))));
    await expect(request("GET", "/x")).rejects.toMatchObject({ code: NETWORK_ERROR });
  });

  it("tells the shell when the session is rejected", async () => {
    const handler = vi.fn();
    setUnauthorizedHandler(handler);
    vi.stubGlobal("fetch", respond(401, { code: "unauthorized", message: "No." }));
    await expect(request("GET", "/x")).rejects.toBeInstanceOf(ApiFailure);
    expect(handler).toHaveBeenCalledOnce();
    setUnauthorizedHandler(() => undefined);
  });
});

describe("failure detail", () => {
  it("keeps the structured detail", async () => {
    vi.stubGlobal("fetch", respond(409, { code: "already_decided", message: "No.", detail: { id: "a" } }));
    await expect(request("POST", "/x")).rejects.toMatchObject({ detail: { id: "a" } });
  });
});

describe("checks the person asked for", () => {
  it("send ?refresh=1 for the workspace and the update status, and only then", async () => {
    const fetchMock = respond(200, {});
    vi.stubGlobal("fetch", fetchMock);
    const { api } = await import("./client");
    await api.workspace(undefined, true);
    await api.workspace();
    await api.updates(undefined, true);
    await api.updates();
    const urls = (fetchMock.mock.calls as unknown as [string][]).map(([url]) => url);
    expect(urls[0]).toContain("/api/workspace?refresh=1");
    expect(urls[1]).not.toContain("refresh");
    expect(urls[2]).toContain("/api/updates?refresh=1");
    expect(urls[3]).not.toContain("refresh");
  });
});

describe("api surface", () => {
  it("has no per-file restore of a turn; only the whole-turn restore and the rewind remain", async () => {
    const { api } = await import("./client");
    expect(Object.keys(api)).not.toContain("restoreChanges");
    expect(Object.keys(api)).toContain("restoreWorkspace");
  });
});

describe("commits", () => {
  it("names the repository in the query only when one is chosen", async () => {
    const fetchMock = respond(200, []);
    vi.stubGlobal("fetch", fetchMock);
    await api.commits("t/1");
    await api.commits("t1", undefined, "r 2");
    const urls = fetchMock.mock.calls.map((call) => (call as unknown as [string])[0].replace(/^.*?(?=\/api)/, ""));
    expect(urls).toEqual(["/api/threads/t%2F1/commits", "/api/threads/t1/commits?repo_id=r%202"]);
  });
});

describe("repo_id on thread Git calls", () => {
  const sent = (fetchMock: ReturnType<typeof respond>) =>
    fetchMock.mock.calls.map((call) => {
      const [url, init] = call as unknown as [string, RequestInit];
      return [init.method, url.replace(/^.*?(?=\/api)/, ""), init.body === undefined ? undefined : JSON.parse(String(init.body))];
    });

  it("puts repo_id in the query of the reads (diff, branches) and keeps the scope", async () => {
    const fetchMock = respond(200, []);
    vi.stubGlobal("fetch", fetchMock);
    await api.diff("t1");
    await api.diff("t1", undefined, "worktree");
    await api.diff("t1", undefined, "worktree", "r 2");
    await api.diff("t1", undefined, undefined, "r2");
    await api.branches("t1");
    await api.branches("t1", undefined, "r2");
    expect(sent(fetchMock).map(([method, url]) => [method, url])).toEqual([
      ["GET", "/api/threads/t1/diff"],
      ["GET", "/api/threads/t1/diff?scope=worktree"],
      ["GET", "/api/threads/t1/diff?scope=worktree&repo_id=r%202"],
      ["GET", "/api/threads/t1/diff?repo_id=r2"],
      ["GET", "/api/threads/t1/branches"],
      ["GET", "/api/threads/t1/branches?repo_id=r2"],
    ]);
  });

  it("puts repo_id in the body of the branch writes and the git action, only when one is chosen", async () => {
    const fetchMock = respond(200, { phases: [], notice: null });
    vi.stubGlobal("fetch", fetchMock);
    await api.createBranch("t1", "feat/x");
    await api.createBranch("t1", "feat/x", "r2");
    await api.switchBranch("t1", "main");
    await api.switchBranch("t1", "main", "r2");
    await api.gitAction("t1", { push: true, repo_id: "r2" });
    expect(sent(fetchMock)).toEqual([
      ["POST", "/api/threads/t1/branches", { name: "feat/x" }],
      ["POST", "/api/threads/t1/branches", { name: "feat/x", repo_id: "r2" }],
      ["POST", "/api/threads/t1/branches/switch", { name: "main" }],
      ["POST", "/api/threads/t1/branches/switch", { name: "main", repo_id: "r2" }],
      ["POST", "/api/threads/t1/git/action", { push: true, repo_id: "r2" }],
    ]);
  });
});

describe("run purpose", () => {
  it("puts purpose on the start body only when a run asks for Orbyn's schema", async () => {
    const fetchMock = respond(200, { id: "r1" });
    vi.stubGlobal("fetch", fetchMock);
    await api.startRun("t1", "why is it slow?");
    await api.startRun("t1", "why is it slow?", [], { purpose: "terminal_proposal" });
    const bodies = fetchMock.mock.calls.map((call) => {
      const [, init] = call as unknown as [string, RequestInit];
      return JSON.parse(String(init.body)) as unknown;
    });
    expect(bodies).toEqual([
      { prompt: "why is it slow?" },
      { prompt: "why is it slow?", purpose: "terminal_proposal" },
    ]);
  });
});
