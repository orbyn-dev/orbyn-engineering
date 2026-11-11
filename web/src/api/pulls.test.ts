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
