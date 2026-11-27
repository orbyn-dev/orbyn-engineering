import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { forgetPersonKey, PERSON_HEADER } from "../../api/personKey";
import { createHttpIssuesApi, createLiveIssuesApi } from "./http";
import type { IssueQuery } from "./contract";

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

  it("fills what an older detail leaves out, and has no `states` when the service sent no list", async () => {
    service(() => json({ ...REF, body: "b", transitions: [{ category: "closed", name: "Closed" }] }));
    const detail = await createHttpIssuesApi().get(REF);
    expect(detail.comments).toEqual([]);
    expect(detail.linked_pull_requests).toEqual([]);
    expect(detail.linked_threads).toEqual([]);
    // The old mock-only word is not the contract's: only `states` says a state change is offered.
    expect(detail.states).toBeUndefined();
  });

  it("keeps only well-formed states, with capped names, and `[]` when the service offers none", async () => {
    service(() => json({ ...REF, body: "b", states: [{ category: "open", name: "Open" }, { category: "closed", name: "x".repeat(500), close_reason: "not_planned" }, { category: "weird", name: "Bad" }, { category: "open" }, null, "x"] }));
    const detail = await createHttpIssuesApi().get(REF);
    expect(detail.states).toEqual([{ category: "open", name: "Open" }, { category: "closed", name: "x".repeat(80), close_reason: "not_planned" }]);
    service(() => json({ ...REF, body: "b", states: [] }));
    expect((await createHttpIssuesApi().get(REF)).states).toEqual([]);
    service(() => json({ ...REF, body: "b", states: "Closed" }));
    expect((await createHttpIssuesApi().get(REF)).states).toBeUndefined();
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

  it("sends the sort only when one was chosen, and the labels as repeated `label`", async () => {
    service(() => json({ issues: [], counts: null, failed: [], next_cursors: {} }));
    const query: IssueQuery = { tracker: null, state: "open", labels: ["bug", "good first"], assignee: "none", text: "" };
    await createHttpIssuesApi().list(query);
    await createHttpIssuesApi().list({ ...query, sort: "created" });
    expect(calls[0]!.url.searchParams.has("sort")).toBe(false);
    expect(calls[1]!.url.searchParams.get("sort")).toBe("created");
    expect(calls[1]!.url.searchParams.getAll("label")).toEqual(["bug", "good first"]);
  });

  it("writes the update with the person key: the ref and only the fields given, as values in a JSON body", async () => {
    service(() => json({ ...REF, body: "new", states: [{ category: "open", name: "Open" }] }));
    const detail = await createHttpIssuesApi().update(REF, { body: "a\"b </script> <<<ISSUE TEXT", labels: ["bug"] });
    expect(calls).toHaveLength(1);
    expect(`${calls[0]!.method} ${calls[0]!.url.pathname}`).toBe("POST /api/issues/item/update");
    expect(calls[0]!.url.search).toBe("");
    expect(calls[0]!.headers.get(PERSON_HEADER)).toBe("shell-key");
    expect(JSON.parse(calls[0]!.body!)).toEqual({ ...REF, body: "a\"b </script> <<<ISSUE TEXT", labels: ["bug"] });
    expect(detail.comments).toEqual([]);
    expect(detail.states).toEqual([{ category: "open", name: "Open" }]);
  });

  it("refuses an update over a limit, or with nothing in it, before anything is sent", async () => {
    service(() => json({ ...REF, body: "b" }));
    const api = createHttpIssuesApi();
    await expect(api.update(REF, {})).rejects.toThrow(/nothing to change/i);
    await expect(api.update(REF, { title: "a\nb" })).rejects.toThrow(/one line/i);
    await expect(api.update(REF, { title: "x".repeat(257) })).rejects.toThrow(/256/);
    await expect(api.update(REF, { body: "é".repeat(33_000) })).rejects.toThrow(/64 KiB/);
    await expect(api.update(REF, { labels: Array.from({ length: 51 }, (_, at) => `l${at}`) })).rejects.toThrow(/50 labels/);
    await expect(api.update(REF, { assignees: Array.from({ length: 26 }, (_, at) => `u${at}`) })).rejects.toThrow(/25 assignees/);
    expect(calls).toEqual([]);
    // At the limits it is sent.
    await api.update(REF, { title: "x".repeat(256), body: "a".repeat(65_536), labels: Array.from({ length: 50 }, (_, at) => `l${at}`), assignees: Array.from({ length: 25 }, (_, at) => `u${at}`) });
    expect(calls).toHaveLength(1);
  });

  it("writes the state with the person key as the state's name", async () => {
    service(() => json({ ...REF, body: "b", states: [] }));
    await createHttpIssuesApi().setState(REF, { category: "closed", name: "Closed", close_reason: "completed" });
    expect(`${calls[0]!.method} ${calls[0]!.url.pathname}`).toBe("POST /api/issues/item/state");
    expect(calls[0]!.headers.get(PERSON_HEADER)).toBe("shell-key");
    expect(JSON.parse(calls[0]!.body!)).toEqual({ ...REF, state: "Closed", close_reason: "completed" });
  });

  it("sends close_reason with a state that has one, so GitHub's two Closed states do not collide, and omits it otherwise", async () => {
    service(() => json({ ...REF, body: "b", states: [] }));
    const api = createHttpIssuesApi();
    await api.setState(REF, { category: "closed", name: "Closed", close_reason: "not_planned" });
    await api.setState(REF, { category: "open", name: "Open" });
    await api.setState(REF, { category: "open", name: "Open", close_reason: null });
    expect(JSON.parse(calls[0]!.body!)).toEqual({ ...REF, state: "Closed", close_reason: "not_planned" });
    expect(JSON.parse(calls[1]!.body!)).toEqual({ ...REF, state: "Open" });
    expect(JSON.parse(calls[2]!.body!)).toEqual({ ...REF, state: "Open" });
  });

  it("writes a comment with the person key and the body as a value, and keeps what the service answered", async () => {
    service(() => json({ id: "IC_1", author: { login: "sam" }, body: "a\"b </script>", created_at: "2026-11-20T10:00:00Z" }));
    const stored = await createHttpIssuesApi().comment(REF, "a\"b </script> <<<ISSUE TEXT");
    expect(calls).toHaveLength(1);
    expect(`${calls[0]!.method} ${calls[0]!.url.pathname}`).toBe("POST /api/issues/item/comment");
    expect(calls[0]!.url.search).toBe("");
    expect(calls[0]!.headers.get(PERSON_HEADER)).toBe("shell-key");
    expect(JSON.parse(calls[0]!.body!)).toEqual({ ...REF, body: "a\"b </script> <<<ISSUE TEXT" });
    expect(stored).toEqual({ id: "IC_1", author: { login: "sam" }, body: "a\"b </script>", created_at: "2026-11-20T10:00:00Z" });
    // Anything but a comment keeps the page from crashing: empty fields, no author.
    service(() => json("nonsense"));
    expect(await createHttpIssuesApi().comment(REF, "x")).toEqual({ id: "", author: null, body: "", created_at: "" });
  });

  it("refuses a blank comment or one over 64 KiB before anything is sent", async () => {
    service(() => json({ id: "c1", author: null, body: "b", created_at: "t" }));
    const api = createHttpIssuesApi();
    await expect(api.comment(REF, "")).rejects.toThrow(/empty/i);
    await expect(api.comment(REF, "   \n ")).rejects.toThrow(/empty/i);
    await expect(api.comment(REF, "é".repeat(33_000))).rejects.toThrow(/64 KiB/);
    expect(calls).toEqual([]);
    // At the limits it is sent.
    await api.comment(REF, "a".repeat(65_536));
    expect(calls).toHaveLength(1);
  });

  it("reads the candidates for a scope, capped, with no entry a write could not use", async () => {
    const many = Array.from({ length: 260 }, (_, at) => ({ name: `l${at}` }));
    service(() => json({ labels: [...many, { color: "red" }], assignees: [{ login: "riley", id: 7, name: "R".repeat(300) }, { login: "no-id" }, { id: "x" }], truncated: false }));
    const found = await createHttpIssuesApi().candidates(REF);
    expect(`${calls[0]!.method} ${calls[0]!.url.pathname}`).toBe("GET /api/issues/candidates");
    expect(Object.fromEntries(calls[0]!.url.searchParams)).toEqual({ tracker: "github", scope: "orbyn-dev/orbyn-engineering" });
    expect(found.labels).toHaveLength(200);
    expect(found.assignees).toEqual([{ login: "riley", id: "7", name: "R".repeat(80) }]);
    // More than 200 arrived, so the list is cut short whatever the service said.
    expect(found.truncated).toBe(true);
    service(() => json("nonsense"));
    expect(await createHttpIssuesApi().candidates(REF)).toEqual({ labels: [], assignees: [], truncated: false });
  });

  it("reads the tracker's templates for a scope, with string labels, capped bodies and no entry a create cannot name", async () => {
    service(() => json([
      { id: "bug", name: "Bug report", description: "Something is broken", body: "Steps to reproduce:", labels: ["bug", 7, ""] },
      { id: "no-name", body: "x" },
      { id: "feature", name: "Feature", body: "y".repeat(20_000), labels: "nope" },
      "nonsense",
    ]));
    const found = await createHttpIssuesApi().templates(REF);
    expect(`${calls[0]!.method} ${calls[0]!.url.pathname}`).toBe("GET /api/issues/templates");
    expect(Object.fromEntries(calls[0]!.url.searchParams)).toEqual({ tracker: "github", scope: "orbyn-dev/orbyn-engineering" });
    expect(calls[0]!.headers.has(PERSON_HEADER)).toBe(false);
    expect(found).toEqual([
      { id: "bug", name: "Bug report", description: "Something is broken", body: "Steps to reproduce:", labels: ["bug"] },
      { id: "feature", name: "Feature", body: "y".repeat(16 * 1024), labels: [] },
    ]);
    // Anything but a list is a tracker with no templates, not a failed read.
    service(() => json("nonsense"));
    expect(await createHttpIssuesApi().templates(REF)).toEqual([]);
  });

  it("writes the create with the person key: the title, the fields given and the whole sets, and answers the stored issue", async () => {
    service(() => json({ ...REF, key: "500", display_key: "#500", body: "b", reactions: [{ content: "HEART", count: 2 }], comments: [] }));
    const api = createHttpIssuesApi();
    const created = await api.create({ tracker: "github", scope: "orbyn-dev/orbyn-engineering", title: "A title", body: "A body", labels: ["bug"], assignees: ["u-1"], template: "bug" });
    expect(`${calls[0]!.method} ${calls[0]!.url.pathname}`).toBe("POST /api/issues/item/create");
    expect(calls[0]!.url.search).toBe("");
    expect(calls[0]!.headers.get(PERSON_HEADER)).toBe("shell-key");
    expect(JSON.parse(calls[0]!.body!)).toEqual({ tracker: "github", scope: "orbyn-dev/orbyn-engineering", title: "A title", labels: ["bug"], assignees: ["u-1"], body: "A body", template: "bug" });
    expect(created.comments).toEqual([]);
    expect(created.reactions).toEqual([{ content: "HEART", count: 2 }]);
    // The fields it leaves out are absent, and the sets travel as empty lists, as the contract defaults them.
    service(() => json({ ...REF, body: "b" }));
    await api.create({ tracker: "github", scope: "o/r", title: "T" });
    expect(JSON.parse(calls[0]!.body!)).toEqual({ tracker: "github", scope: "o/r", title: "T", labels: [], assignees: [] });
  });

  it("refuses a create with no title or over a limit before anything is sent", async () => {
    service(() => json({ ...REF, body: "b" }));
    const api = createHttpIssuesApi();
    const draft = { tracker: "github", scope: "o/r", title: "T" } as const;
    await expect(api.create({ ...draft, title: "" })).rejects.toThrow(/empty/i);
    await expect(api.create({ ...draft, title: "   " })).rejects.toThrow(/empty/i);
    await expect(api.create({ ...draft, title: "a\nb" })).rejects.toThrow(/one line/i);
    await expect(api.create({ ...draft, title: "x".repeat(257) })).rejects.toThrow(/256/);
    await expect(api.create({ ...draft, body: "é".repeat(33_000) })).rejects.toThrow(/64 KiB/);
    await expect(api.create({ ...draft, labels: Array.from({ length: 51 }, (_, at) => `l${at}`) })).rejects.toThrow(/50 labels/);
    await expect(api.create({ ...draft, assignees: Array.from({ length: 26 }, (_, at) => `u${at}`) })).rejects.toThrow(/25 assignees/);
    expect(calls).toEqual([]);
    // At the limits it is sent.
    await api.create({ ...draft, title: "x".repeat(256), body: "a".repeat(65_536), labels: Array.from({ length: 50 }, (_, at) => `l${at}`), assignees: Array.from({ length: 25 }, (_, at) => `u${at}`) });
    expect(calls).toHaveLength(1);
  });

  it("writes a reaction with the person key and answers the issue's counts, dropping ones that are not a count", async () => {
    service(() => json({ ...REF, body: "b", reactions: [{ content: "THUMBS_UP", count: 3 }, { content: "", count: 2 }, { content: "HEART", count: -1 }, { content: "ROCKET", count: 1.5 }], comments: [] }));
    const detail = await createHttpIssuesApi().react(REF, "thumbs_up", true);
    expect(`${calls[0]!.method} ${calls[0]!.url.pathname}`).toBe("POST /api/issues/item/react");
    expect(calls[0]!.url.search).toBe("");
    expect(calls[0]!.headers.get(PERSON_HEADER)).toBe("shell-key");
    expect(JSON.parse(calls[0]!.body!)).toEqual({ ...REF, reaction: "thumbs_up", on: true });
    expect(detail.reactions).toEqual([{ content: "THUMBS_UP", count: 3 }, { content: "ROCKET", count: 1 }]);
    // A write answer that reports no reactions leaves them unknown, never "none".
    service(() => json({ ...REF, body: "b", reactions: null }));
    expect((await createHttpIssuesApi().react(REF, "eyes", false)).reactions).toBeUndefined();
  });

  it("offers() reads once, and only a 404 or 405 that is not a missing issue says no", async () => {
    service(() => json({ code: "invalid", message: "tracker is required" }, 400));
    expect(await createHttpIssuesApi().offers()).toBe(true);
    service(() => json({ code: "tracker_not_connected", message: "m" }, 409));
    expect(await createHttpIssuesApi().offers()).toBe(true);
    service(() => new Response("", { status: 404 }));
    expect(await createHttpIssuesApi().offers()).toBe(false);
    service(() => new Response("", { status: 405 }));
    expect(await createHttpIssuesApi().offers()).toBe(false);
    service(() => { throw new TypeError("offline"); });
    await expect(createHttpIssuesApi().offers()).rejects.toThrow(/reach the Orbyn service/);
    // Every probe is a read: nothing was written.
    expect(calls.every((call) => call.method === "GET")).toBe(true);
  });

  it("pages with `cursors`: once, JSON then URL-encoded, with the same filters", async () => {
    service(() => json({ issues: [], counts: null, failed: [], next_cursors: {} }));
    const query: IssueQuery = { tracker: "github", state: "open", labels: ["bug"], assignee: "me", text: "x" };
    await createHttpIssuesApi().list(query, undefined, { "github o/r": "abc", "linear Orbyn team": "z9" });
    const raw = calls[0]!.url.search;
    expect(raw).toContain("cursors=%7B%22github%20o%2Fr%22%3A%22abc%22%2C%22linear%20Orbyn%20team%22%3A%22z9%22%7D");
    expect(calls[0]!.url.searchParams.getAll("cursors")).toHaveLength(1);
    expect(JSON.parse(calls[0]!.url.searchParams.get("cursors")!)).toEqual({ "github o/r": "abc", "linear Orbyn team": "z9" });
    expect(calls[0]!.url.searchParams.get("state")).toBe("open");
    expect(calls[0]!.url.searchParams.getAll("label")).toEqual(["bug"]);
    expect(calls[0]!.url.searchParams.get("assignee")).toBe("me");
    expect(calls[0]!.url.searchParams.get("q")).toBe("x");
    // No cursors, no parameter. An empty object is no paging either.
    await createHttpIssuesApi().list(query);
    await createHttpIssuesApi().list(query, undefined, {});
    expect(calls[1]!.url.searchParams.has("cursors")).toBe(false);
    expect(calls[2]!.url.searchParams.has("cursors")).toBe(false);
  });

  it("refuses a cursors value over 4 KiB before anything is sent", async () => {
    service(() => json({ issues: [], counts: null }));
    const query: IssueQuery = { tracker: null, state: "open", labels: [], assignee: "any", text: "" };
    await expect(createHttpIssuesApi().list(query, undefined, { "github o/r": "x".repeat(4200) })).rejects.toThrow(/too many pages/i);
    expect(calls).toEqual([]);
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
    await expect(api.startThread(REF, { message: "m", project_id: "p" })).rejects.toThrow(/not reported/);
    await expect(api.connect("gitlab", { token: TOKEN })).rejects.toThrow(/not reported/);
    expect(calls.map((call) => `${call.method} ${call.url.pathname}`)).toEqual(["GET /api/issues/trackers"]);
  });

  it("offers the writes only after the trackers answer and the probe says so, and caches the answer; a network failure asks again", async () => {
    let candidates = 404;
    service((call) => (call.url.pathname === "/api/issues/trackers" ? json([{ tracker: "github", connected: true }]) : json({ code: "x", message: "m" }, candidates)));
    const api = createLiveIssuesApi();
    expect(await api.offers()).toBe(false);
    candidates = 400;
    // Cached: the first answer stands for the life of the page.
    expect(await api.offers()).toBe(false);
    expect(calls.map((call) => call.url.pathname)).toEqual(["/api/issues/trackers", "/api/issues/candidates"]);
    service((call) => (call.url.pathname === "/api/issues/trackers" ? json([{ tracker: "github", connected: true }]) : json({ code: "invalid", message: "m" }, 400)));
    expect(await createLiveIssuesApi().offers()).toBe(true);
    service(() => json({ code: "not_found", message: "Not found." }, 404));
    const absent = createLiveIssuesApi();
    expect(await absent.offers()).toBe(false);
    await expect(absent.update(REF, { body: "x" })).rejects.toThrow(/not reported/);
    await expect(absent.setState(REF, { category: "open", name: "Open" })).rejects.toThrow(/not reported/);
    await expect(absent.candidates(REF)).rejects.toThrow(/not reported/);
    await expect(absent.comment(REF, "x")).rejects.toThrow(/not reported/);
    await expect(absent.create({ tracker: "github", scope: "o/r", title: "T" })).rejects.toThrow(/not reported/);
    await expect(absent.react(REF, "thumbs_up", true)).rejects.toThrow(/not reported/);
    // The templates read is quiet: a service that reports no issues has none, so the dialog draws no Template row.
    await expect(absent.templates(REF)).resolves.toEqual([]);
    expect(calls.filter((call) => call.method !== "GET")).toEqual([]);
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
