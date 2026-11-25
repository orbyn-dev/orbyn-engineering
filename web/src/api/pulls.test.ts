import { afterEach, describe, expect, it, vi } from "vitest";
import { pullsApi } from "./pulls";

afterEach(() => vi.unstubAllGlobals());

describe("pullsApi.details", () => {
  it("posts the listed rows and whether the refresh was the person's own", async () => {
    const fetchMock = vi.fn(async () => new Response(JSON.stringify({ details: [], paused: [] }), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    const body = { pulls: [{ project_id: "p", number: 3, updated_at: "t" }], fresh: true };
    await expect(pullsApi.details(body)).resolves.toEqual({ details: [], paused: [] });
    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toContain("/api/pulls/details");
    expect(init.method).toBe("POST");
    expect(JSON.parse(init.body as string)).toEqual(body);
  });
});

describe("a refresh the person pressed", () => {
  const urls = () => {
    const fetchMock = vi.fn(async () => new Response("[]", { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    return () => (fetchMock.mock.calls as unknown as [string][]).map(([url]) => url);
  };

  it("asks for a real read of the list, the detail and the forges", async () => {
    const seen = urls();
    await pullsApi.list("open", "", undefined, undefined, true);
    await pullsApi.list("open", "", undefined, undefined, false);
    await pullsApi.detail("p", 3, undefined, undefined, true);
    await pullsApi.forges(undefined, true);
    const [forced, plain, detail, forges] = seen();
    expect(forced).toContain("&refresh=1");
    expect(plain).not.toContain("refresh");
    expect(detail).toContain("/pulls/3?refresh=1");
    expect(forges).toContain("/api/forges?rescan=true");
  });
});

describe("pullsApi.creditsSetting", () => {
  const read = async (rows: unknown) => {
    const fetchMock = vi.fn(async () => new Response(JSON.stringify(rows), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    const value = await pullsApi.creditsSetting("p 1");
    return { value, url: (fetchMock.mock.calls[0] as unknown as [string])[0] };
  };

  it("reads the setting as it applies to the project", async () => {
    const { value, url } = await read([{ key: "remove_agent_credits_on_merge", value: true }]);
    expect(value).toBe(true);
    expect(url).toContain("/api/preferences?project_id=p%201");
  });

  it("says nothing is known when the service does not list it", async () => {
    expect((await read([{ key: "other", value: true }])).value).toBeNull();
    expect((await read([{ key: "remove_agent_credits_on_merge", value: "yes" }])).value).toBeNull();
  });
});

describe("the thread pull request calls and repo_id", () => {
  const calls = () => {
    const fetchMock = vi.fn(async () => new Response("[]", { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    return () => (fetchMock.mock.calls as unknown as [string, RequestInit][]).map(([url, init]) => [init.method, url.replace(/^.*?\/api/, "/api")]);
  };

  it("sends no repo_id when none is given, byte for byte as before", async () => {
    const seen = calls();
    await pullsApi.linked("t 1");
    await pullsApi.refreshLinked("t 1");
    await pullsApi.link("t 1", 4);
    await pullsApi.unlink("t 1", 4);
    await pullsApi.threadReview("t 1", 4, "approve", "ok");
    await pullsApi.threadMerge("t 1", 4, { method: "squash", auto: false, delete_branch: false, strip_credits: false });
    await pullsApi.watches("t 1");
    await pullsApi.watch("t 1", 4);
    await pullsApi.unwatch("t 1", 4);
    expect(seen()).toEqual([
      ["GET", "/api/threads/t%201/pulls"],
      ["POST", "/api/threads/t%201/pulls/refresh"],
      ["POST", "/api/threads/t%201/pulls"],
      ["DELETE", "/api/threads/t%201/pulls/4"],
      ["POST", "/api/threads/t%201/pulls/4/review"],
      ["POST", "/api/threads/t%201/pulls/4/merge"],
      ["GET", "/api/threads/t%201/watches"],
      ["POST", "/api/threads/t%201/pulls/4/watch"],
      ["DELETE", "/api/threads/t%201/pulls/4/watch"],
    ]);
  });

  it("sends the repository as ?repo_id= on each of them", async () => {
    const seen = calls();
    await pullsApi.linked("t", undefined, "r/1");
    await pullsApi.refreshLinked("t", "r/1");
    await pullsApi.link("t", 4, "r/1");
    await pullsApi.unlink("t", 4, "r/1");
    await pullsApi.threadReview("t", 4, "approve", "", "r/1");
    await pullsApi.threadMerge("t", 4, { method: "merge", auto: false, delete_branch: false, strip_credits: false, remove_agent_credits: true }, "r/1");
    await pullsApi.watches("t", undefined, "r/1");
    await pullsApi.watch("t", 4, "r/1");
    await pullsApi.unwatch("t", 4, "r/1");
    expect(seen().map(([, url]) => url)).toEqual([
      "/api/threads/t/pulls?repo_id=r%2F1",
      "/api/threads/t/pulls/refresh?repo_id=r%2F1",
      "/api/threads/t/pulls?repo_id=r%2F1",
      "/api/threads/t/pulls/4?repo_id=r%2F1",
      "/api/threads/t/pulls/4/review?repo_id=r%2F1",
      "/api/threads/t/pulls/4/merge?repo_id=r%2F1",
      "/api/threads/t/watches?repo_id=r%2F1",
      "/api/threads/t/pulls/4/watch?repo_id=r%2F1",
      "/api/threads/t/pulls/4/watch?repo_id=r%2F1",
    ]);
  });
});
