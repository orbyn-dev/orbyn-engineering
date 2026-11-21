import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { forgetPersonKey, PERSON_HEADER } from "../../api/personKey";
import { createHttpIssuesApi, createLiveIssuesApi } from "./http";

const TOKEN = "glpat-SECRET-0123456789";
const REF = { tracker: "github", scope: "orbyn-dev/orbyn-engineering", key: "412" } as const;

type Call = { url: URL; method: string; headers: Headers; body: string | undefined };
let calls: Call[] = [];
const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });

/** Answers each request with `reply(call)`, and records it. */
function service(reply: (call: Call) => Response | Promise<Response>) {
  calls = [];
  vi.stubGlobal("fetch", vi.fn(async (url: string, init: RequestInit) => {
    const call: Call = { url: new URL(url, "http://service.test"), method: init.method ?? "GET", headers: new Headers(init.headers), body: typeof init.body === "string" ? init.body : undefined };
    calls.push(call);
    return reply(call);
  }));
}

beforeEach(() => {
  // The shell's person key, as the desktop app hands it over.
  (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke: async () => "shell-key" };
});
afterEach(() => {
  delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  forgetPersonKey();
  vi.unstubAllGlobals();
});

describe("the routes", () => {
  it("reads the trackers, and a 404 is null: this service reports no issues", async () => {
    service(() => json({ code: "not_found", message: "Not found." }, 404));
    expect(await createHttpIssuesApi().trackers()).toBeNull();
    expect(calls.map((call) => `${call.method} ${call.url.pathname}`)).toEqual(["GET /api/issues/trackers"]);
    service(() => json([{ tracker: "github", connected: false, detail: "gh is not installed" }]));
    expect(await createHttpIssuesApi().trackers()).toEqual([{ tracker: "github", connected: false, detail: "gh is not installed" }]);
    // Anything that is not a list is not this service's answer.
    service(() => json({ ok: true }));
    expect(await createHttpIssuesApi().trackers()).toBeNull();
  });

  it("does not read any other failure as 'no issues'", async () => {
    service(() => json({ code: "internal", message: "Broke." }, 500));
    await expect(createHttpIssuesApi().trackers()).rejects.toMatchObject({ status: 500 });
  });

  it("lists with the filters in the query, one label parameter each, and no key", async () => {
    service(() => json({ issues: [], counts: null }));
    const list = await createHttpIssuesApi().list({ tracker: "linear", state: "closed", labels: ["bug", "good first"], assignee: "me", text: "icon picker" });
    expect(calls[0]!.method).toBe("GET");
    expect(calls[0]!.url.pathname).toBe("/api/issues");
    expect(calls[0]!.url.searchParams.get("tracker")).toBe("linear");
    expect(calls[0]!.url.searchParams.get("state")).toBe("closed");
    expect(calls[0]!.url.searchParams.getAll("label")).toEqual(["bug", "good first"]);
    expect(calls[0]!.url.searchParams.get("assignee")).toBe("me");
    expect(calls[0]!.url.searchParams.get("q")).toBe("icon picker");
    expect(calls[0]!.headers.has(PERSON_HEADER)).toBe(false);
    // The pagination fields are present even where the service leaves them out.
    expect(list).toEqual({ issues: [], counts: null, failed: [], next_cursors: {} });
    service(() => json({ issues: [], counts: null, failed: [], next_cursors: { "github:o/r": "abc" } }));
    expect((await createHttpIssuesApi().list({ tracker: null, state: "open", labels: [], assignee: "any", text: "" })).next_cursors).toEqual({ "github:o/r": "abc" });
    expect(calls[0]!.url.searchParams.has("tracker")).toBe(false);
    expect(calls[0]!.url.searchParams.has("assignee")).toBe(false);
  });

  it("names the whole ref in the query of an item, a draft, and puts nothing of it in the path", async () => {
    service(() => json({ message: "m", ...REF, body: "b", comments: [] }));
    await createHttpIssuesApi().get({ ...REF, key: "ORB 87/#1" });
    await createHttpIssuesApi().draft(REF);
    expect(calls.map((call) => call.url.pathname)).toEqual(["/api/issues/item", "/api/issues/draft"]);
    expect(Object.fromEntries(calls[0]!.url.searchParams)).toEqual({ tracker: "github", scope: "orbyn-dev/orbyn-engineering", key: "ORB 87/#1" });
    expect(Object.fromEntries(calls[1]!.url.searchParams)).toEqual({ tracker: "github", scope: "orbyn-dev/orbyn-engineering", key: "412" });
  });

  it("fills what an older detail leaves out, and drops transitions, which are not in the contract", async () => {
    service(() => json({ ...REF, body: "b", transitions: [{ category: "closed", name: "Closed" }] }));
    const detail = await createHttpIssuesApi().get(REF);
    expect(detail.comments).toEqual([]);
    expect(detail.linked_pull_requests).toEqual([]);
    expect(detail.linked_threads).toEqual([]);
    expect(detail.transitions).toBeUndefined();
  });

  it("reads the links and the find with the words as one query value", async () => {
    service((call) => json(call.url.pathname === "/api/issues/links" ? { t1: [] } : []));
    expect(await createHttpIssuesApi().links()).toEqual({ t1: [] });
    await createHttpIssuesApi().find("a&b=c #412");
    expect(calls[1]!.url.pathname).toBe("/api/issues/find");
    expect(calls[1]!.url.searchParams.get("q")).toBe("a&b=c #412");
  });

  it("links, unlinks and starts with the key, the three names of the issue and nothing else, as a person write", async () => {
    service((call) => (call.url.pathname === "/api/issues/start-thread" ? json({ id: "t9" }) : new Response(null, { status: 204 })));
    const api = createHttpIssuesApi();
    const extra = { ...REF, title: "not sent", labels: [] };
    await api.link("thread/1", extra);
    await api.unlink("thread/1", extra);
    const thread = await api.startThread(extra, { message: "Work on it", project_id: "p1" });
    expect(thread).toEqual({ id: "t9" });
    expect(calls.map((call) => `${call.method} ${call.url.pathname}`)).toEqual([
      "POST /api/threads/thread%2F1/issue-links", "POST /api/threads/thread%2F1/issue-links/remove", "POST /api/issues/start-thread",
    ]);
    expect(JSON.parse(calls[0]!.body!)).toEqual(REF);
    expect(JSON.parse(calls[1]!.body!)).toEqual(REF);
    expect(JSON.parse(calls[2]!.body!)).toEqual({ ...REF, message: "Work on it", project_id: "p1" });
    for (const call of calls) expect(call.headers.get(PERSON_HEADER), call.url.pathname).toBe("shell-key");
  });

  it("connects with the token in the JSON body only, never in the path, the query or a header, and disconnects", async () => {
    service(() => json({ tracker: "gitlab", connected: true, host: "https://gitlab.com" }));
    const api = createHttpIssuesApi();
    const row = await api.connect("gitlab", { host: "https://gitlab.com", token: TOKEN });
    await api.disconnect("gitlab");
    expect(calls.map((call) => `${call.method} ${call.url.pathname}`)).toEqual(["POST /api/issues/trackers/gitlab/connect", "POST /api/issues/trackers/gitlab/disconnect"]);
    expect(JSON.parse(calls[0]!.body!)).toEqual({ host: "https://gitlab.com", token: TOKEN });
    expect(calls[1]!.body).toBeUndefined();
    for (const call of calls) {
      expect(call.url.href).not.toContain(TOKEN);
      expect(call.url.search).toBe("");
      expect([...call.headers.values()].join("|")).not.toContain(TOKEN);
      expect(call.headers.get(PERSON_HEADER)).toBe("shell-key");
    }
    expect(JSON.stringify(row)).not.toContain(TOKEN);
  });

  it("encodes a tracker as one path segment", async () => {
    service(() => json({}));
    await createHttpIssuesApi().connect("a/b?c" as never, {});
    expect(calls[0]!.url.pathname).toBe("/api/issues/trackers/a%2Fb%3Fc/connect");
  });

  it("carries the service's own error, so a screen can say why", async () => {
    service(() => json({ code: "tracker_not_connected", message: "gh is signed out." }, 409));
    await expect(createHttpIssuesApi().link("t", REF)).rejects.toMatchObject({ code: "tracker_not_connected", status: 409, message: "gh is signed out." });
  });

  it("has no state change: the doc names none", async () => {
    await expect(createHttpIssuesApi().setState(REF, { category: "closed", name: "Closed" })).rejects.toThrow();
  });
});

describe("the client the app starts with", () => {
  it("stays silent on a 404: after the one probe it makes no request, answers empty and refuses every write", async () => {
    service(() => json({ code: "not_found", message: "Not found." }, 404));
    const api = createLiveIssuesApi();
    expect(await api.links()).toEqual({});
    expect(await api.find("x")).toEqual([]);
    expect(await api.list({ tracker: null, state: "open", labels: [], assignee: "any", text: "" })).toMatchObject({ issues: [], next_cursors: {} });
    expect(await api.trackers()).toBeNull();
    await expect(api.link("t", REF)).rejects.toThrow("not reported");
    await expect(api.startThread(REF, { message: "m", project_id: "p" })).rejects.toThrow();
    await expect(api.connect("gitlab", { token: TOKEN })).rejects.toThrow();
    expect(calls.map((call) => `${call.method} ${call.url.pathname}`)).toEqual(["GET /api/issues/trackers"]);
  });

  it("turns the real routes on once the trackers answer, and not before", async () => {
    service((call) => (call.url.pathname === "/api/issues/trackers" ? json([{ tracker: "github", connected: true }]) : json({ t1: [] })));
    const api = createLiveIssuesApi();
    expect(await api.links()).toEqual({ t1: [] });
    expect(calls.map((call) => call.url.pathname)).toEqual(["/api/issues/trackers", "/api/issues/links"]);
    expect(await api.trackers()).toEqual([{ tracker: "github", connected: true }]);
  });

  it("does not remember a network failure as 'absent': it asks again", async () => {
    let up = false;
    service((call) => {
      if (!up) throw new TypeError("offline");
      return call.url.pathname === "/api/issues/trackers" ? json([{ tracker: "github", connected: true }]) : json({});
    });
    const api = createLiveIssuesApi();
    expect(await api.links()).toEqual({});
    up = true;
    await api.links();
    expect(calls.some((call) => call.url.pathname === "/api/issues/links")).toBe(true);
  });
});
