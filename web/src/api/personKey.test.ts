import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { terminalUrl } from "../features/panels/terminal/terminalSocket";
import { ApiFailure, request } from "./client";
import { canSendPersonKey, forgetPersonKey, PENDING_PERSON_ROUTES, PERSON_HEADER, PERSON_ROUTES, personKeyHeaders, personKeyProtocols, sendsPersonKey } from "./personKey";

const ok = () => vi.fn(async () => new Response(null, { status: 204 }));
const sentHeaders = (fetchMock: ReturnType<typeof ok>) => new Headers((fetchMock.mock.calls[0] as unknown as [string, RequestInit])[1].headers);

function shell(invoke: (command: string) => unknown) {
  (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke };
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.stubGlobal("__ORBYN_PREVIEWS__", true);
  delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  forgetPersonKey();
});

describe("which requests carry the person key", () => {
  it("names the enforced routes and the field-level ones, and no other", () => {
    expect(sendsPersonKey("POST", "/api/approvals/a1/decide")).toBe(true);
    expect(sendsPersonKey("POST", "/api/projects/p/worktrees/w/file-actions")).toBe(true);
    expect(sendsPersonKey("post", "/api/approvals/a1/decide?x=1")).toBe(true);
    expect(sendsPersonKey("POST", "/api/threads/t1/console")).toBe(true);
    expect(sendsPersonKey("PATCH", "/api/threads/t1")).toBe(true);
    expect(sendsPersonKey("PUT", "/api/threads/t1/files/content")).toBe(true);
    expect(sendsPersonKey("POST", "/api/threads/t1/files/mutate")).toBe(true);
    expect(sendsPersonKey("DELETE", "/api/projects/p/worktrees/w")).toBe(true);
    expect(sendsPersonKey("POST", "/api/threads/t1/vcs/refresh-status")).toBe(false);
    expect(sendsPersonKey("POST", "/api/account/sign-in/digits")).toBe(true);
    expect(sendsPersonKey("POST", "/api/crew/tasks/t1/merge")).toBe(true);
    expect(sendsPersonKey("POST", "/api/crew/autopilot/merge-queue/t1/resolve")).toBe(true);
    expect(sendsPersonKey("PUT", "/api/crew/autopilot/mode")).toBe(true);
    expect(sendsPersonKey("POST", "/api/crew/projects/p1/worker-provider/set")).toBe(true);
    expect(sendsPersonKey("POST", "/api/crew/escapes/mark")).toBe(true);
    expect(sendsPersonKey("POST", "/api/crew/escapes/esc_1/confirm")).toBe(true);
    expect(sendsPersonKey("POST", "/api/crew/escapes/esc_1/dismiss")).toBe(true);
    expect(sendsPersonKey("POST", "/api/crew/escapes/esc_1/gap")).toBe(true);
    expect(sendsPersonKey("POST", "/api/crew/escapes/esc_1/check")).toBe(true);
    expect(sendsPersonKey("POST", "/api/crew/escapes/esc_1/check/accept")).toBe(true);
    expect(sendsPersonKey("POST", "/api/crew/escapes/esc_1/check/approve")).toBe(true);
    expect(sendsPersonKey("POST", "/api/crew/escapes/esc_1/check/retry")).toBe(true);
    expect(sendsPersonKey("POST", "/api/crew/escapes/esc_1/check/withdraw")).toBe(true);
    expect(sendsPersonKey("GET", "/api/crew/escapes/esc_1")).toBe(false);
    expect(sendsPersonKey("GET", "/api/crew/projects/p1/worker-provider")).toBe(false);
    expect(sendsPersonKey("POST", "/api/threads/t1/runs")).toBe(true);
    // The routes that move a checkout (ruling F3).
    expect(sendsPersonKey("POST", "/api/projects/p1/git/init")).toBe(true);
    expect(sendsPersonKey("POST", "/api/threads/t1/branches")).toBe(true);
    expect(sendsPersonKey("POST", "/api/threads/t1/branches/switch")).toBe(true);
    expect(sendsPersonKey("POST", "/api/threads/t1/vcs/refs")).toBe(true);
    expect(sendsPersonKey("POST", "/api/threads/t1/vcs/refs/switch")).toBe(true);
    expect(sendsPersonKey("GET", "/api/threads/t1/branches")).toBe(false);
    // A pull checkout carries the key for both modes; the service reads it only for `worktree: false`.
    expect(sendsPersonKey("POST", "/api/projects/p1/pulls/12/checkout")).toBe(true);
    expect(sendsPersonKey("GET", "/api/projects/p1/pulls/12/checkout")).toBe(false);
    for (const [method, path] of [
      ["GET", "/api/approvals/a1/decide"],
      ["GET", "/api/approvals?state=pending"],
      ["POST", "/api/approvals/a1/decide/extra"],
      ["POST", "/api/approvals//decide"],
      ["POST", "/api/projects/p/worktrees/w/file"],
      ["GET", "/api/threads/t1"],
      ["POST", "/api/threads/t1/queue"],
      ["POST", "/api/threads/t1/steer"],
      ["POST", "/api/threads/t1/git/action"],
      ["GET", "/api/threads/t1/console"],
      ["DELETE", "/api/threads/t1"],
    ] as const) {
      expect(sendsPersonKey(method, path), `${method} ${path}`).toBe(false);
    }
  });

  it("is exactly the service's person rows plus its field-level rows", () => {
    const table = JSON.parse(readFileSync(resolve(import.meta.dirname, "../../../crates/orbynd/tests/routes.json"), "utf8")) as {
      protected: [string, string, string][];
    };
    const listed = new Set(PERSON_ROUTES.map(([method, pattern]) => `${method} ${pattern}`));
    for (const [method, pattern] of PERSON_ROUTES) {
      expect(table.protected.some(([m, p]) => m === method && p === pattern), `${method} ${pattern}`).toBe(true);
    }
    const person = table.protected.filter(([, , kind]) => kind === "person").map(([m, p]) => `${m} ${p}`);
    for (const row of person) expect(listed.has(row), `${row} is a person route the page does not send the key to`).toBe(true);
    // The field-level routes are the only local rows in the list.
    const local = PERSON_ROUTES.filter(([method, pattern]) => table.protected.some(([m, p, kind]) => m === method && p === pattern && kind === "local"));
    expect(local.map(([method, pattern]) => `${method} ${pattern}`).sort()).toEqual(["POST /agui/threads/{id}", "POST /api/projects/{id}/pulls/{number}/checkout", "POST /api/threads/{id}/runs"]);
  });
});

describe("the Crew and Code Mode cancel routes", () => {
  it("send the key on the exact cancel route, whatever the method or the trailing path says", async () => {
    shell(vi.fn(async () => "shell-key"));
    const cases = [
      ["POST", "/api/crew/tasks/t1/cancel", "shell-key"],
      ["POST", "/api/code-runs/r1/cancel", "shell-key"],
      ["POST", "/api/crew/tasks/t1/cancelled", null],
      ["POST", "/api/code-runs/r1/cancel/extra", null],
      ["GET", "/api/crew/tasks/t1/cancel", null],
    ] as const;
    for (const [method, path, expected] of cases) {
      const sent = ok();
      vi.stubGlobal("fetch", sent);
      await request(method, path);
      const [url, init] = sent.mock.calls[0] as unknown as [string, RequestInit];
      expect(sent, `${method} ${path} is one request`).toHaveBeenCalledTimes(1);
      expect(String(url), `${method} ${path}`).toBe(path);
      expect(new Headers(init.headers).get(PERSON_HEADER), `${method} ${path}`).toBe(expected);
    }
  });
});

describe("the Cursor thread routes", () => {
  it("send the key on the override and the acknowledgement, and on no read of the notice", () => {
    expect(sendsPersonKey("POST", "/api/threads/t1/send-without-approvals")).toBe(true);
    expect(sendsPersonKey("POST", "/api/projects/p1/engines/cursor/notice/acknowledge")).toBe(true);
    expect(sendsPersonKey("GET", "/api/projects/p1/engines/cursor/notice")).toBe(false);
    // The old engine-wide path is gone: the notice is per project.
    expect(sendsPersonKey("POST", "/api/engines/cursor/notice/acknowledge")).toBe(false);
    expect(sendsPersonKey("GET", "/api/threads/t1/send-without-approvals")).toBe(false);
    expect(sendsPersonKey("POST", "/api/projects/p1/engines/cursor/notice")).toBe(false);
  });

  it("are Person routes on the service, and no longer pending", () => {
    for (const route of [
      ["POST", "/api/threads/{id}/send-without-approvals"],
      ["POST", "/api/projects/{project_id}/engines/{engine}/notice/acknowledge"],
    ]) {
      expect(PERSON_ROUTES.some(([m, p]) => m === route[0] && p === route[1])).toBe(true);
      expect(PENDING_PERSON_ROUTES.some(([m, p]) => m === route[0] && p === route[1])).toBe(false);
    }
  });

  it("sends the key with the one-turn override and the acknowledgement through the client", async () => {
    shell(vi.fn(async () => "shell-key"));
    const posted = ok();
    vi.stubGlobal("fetch", posted);
    await request("POST", "/api/threads/t1/send-without-approvals", { prompt: "x" });
    expect(sentHeaders(posted).get(PERSON_HEADER)).toBe("shell-key");
    const acknowledged = ok();
    vi.stubGlobal("fetch", acknowledged);
    await request("POST", "/api/projects/p1/engines/cursor/notice/acknowledge");
    expect(sentHeaders(acknowledged).get(PERSON_HEADER)).toBe("shell-key");
  });
});

describe("the Issues writes", () => {
  it("send the key on every write, the five in the route table and the two edits not built yet, and on no read", () => {
    for (const path of ["/api/issues/trackers/github/connect", "/api/issues/trackers/github/disconnect", "/api/issues/start-thread", "/api/threads/t1/issue-links", "/api/threads/t1/issue-links/remove", "/api/issues/item/update", "/api/issues/item/state"]) {
      expect(sendsPersonKey("POST", path), path).toBe(true);
    }
    for (const path of ["/api/issues", "/api/issues/item", "/api/issues/candidates", "/api/issues/links"]) expect(sendsPersonKey("GET", path), path).toBe(false);
    // The five in the first slice are no longer pending: they are in PERSON_ROUTES, from routes.json.
    const pending = PENDING_PERSON_ROUTES.map(([, pattern]) => pattern);
    expect(pending).toEqual(expect.arrayContaining(["/api/issues/item/update", "/api/issues/item/state"]));
    for (const pattern of ["/api/issues/start-thread", "/api/issues/trackers/{tracker}/connect", "/api/threads/{id}/issue-links"]) expect(pending).not.toContain(pattern);
  });
});

describe("the multi-repository writes of docs/multi-repo.md", () => {
  const writes = [
    "/api/projects/p1/repos/add-folder", "/api/projects/p1/repos/create", "/api/projects/p1/repos/clone", "/api/projects/p1/repos/r1/publish",
    "/api/projects/p1/repos/r1/trust", "/api/projects/p1/repos/r1/setup-git", "/api/projects/p1/repos/r1/remove", "/api/projects/p1/repos/r1/clone/cancel",
    "/api/projects/p1/repo-threads", "/api/threads/t1/repos", "/api/projects/p1/repos/scan",
  ];
  it("send the key on every write and on no read", () => {
    for (const path of writes) expect(sendsPersonKey("POST", path), path).toBe(true);
    expect(sendsPersonKey("GET", "/api/projects/p1/repos")).toBe(false);
    expect(sendsPersonKey("GET", "/api/threads/t1/repos")).toBe(false);
    expect(sendsPersonKey("GET", "/api/threads/t1/task-folder")).toBe(false);
    expect(sendsPersonKey("POST", "/api/projects/p1/repos")).toBe(false);
    expect(sendsPersonKey("POST", "/api/projects/p1/repos/r1")).toBe(false);
  });
});

describe("the terminal sockets", () => {
  it("offers the key as a subprotocol beside orbyn.v1, never in the URL", async () => {
    shell(vi.fn(async () => "shell-key"));
    expect(await personKeyProtocols()).toEqual(["orbyn.v1", "orbyn.person.shell-key"]);
  });

  it("offers only orbyn.v1 where there is no key", async () => {
    shell(vi.fn(async () => ""));
    expect(await personKeyProtocols()).toEqual(["orbyn.v1"]);
  });

  it("puts the key in the socket's subprotocols, never in the URL it opens", async () => {
    const key = "KEY-SENTINEL-123";
    shell(vi.fn(async () => key));
    expect(await personKeyProtocols()).toContain(`orbyn.person.${key}`);
    // The socket-opening effect is not exercised here: it would need the xterm renderer and a browser WebSocket stand-in, and it
    // composes exactly these two helpers, `terminalUrl(...)` and `personKeyProtocols()`. So this pins the helpers only.
    sessionStorage.clear();
    const url = terminalUrl("t1", { protocol: "https:", host: "desk.local" });
    expect(url).toBe("wss://desk.local/ws/terminal/t1");
    expect(url).not.toContain(key);
  });

  it("offers an empty list of person-key subprotocols where the page has no key", async () => {
    shell(vi.fn(async () => ""));
    // `orbyn.v1` still goes so the service selects it; the list of person-key subprotocols is empty.
    expect((await personKeyProtocols()).filter((protocol) => protocol.startsWith("orbyn.person."))).toEqual([]);
  });
});

describe("the key, from the shell", () => {
  it("asks the shell's person_key command once and sends the key as x-orbyn-person on a person route only", async () => {
    const invoke = vi.fn(async () => "shell-key");
    shell(invoke);
    const person = ok();
    vi.stubGlobal("fetch", person);
    await request("POST", "/api/approvals/a1/decide", { allow: true, remember: false });
    expect(sentHeaders(person).get(PERSON_HEADER)).toBe("shell-key");
    expect(invoke).toHaveBeenCalledWith("person_key");

    const other = ok();
    vi.stubGlobal("fetch", other);
    await request("POST", "/api/threads/t1/queue", {});
    await request("GET", "/api/approvals?state=pending");
    for (const call of other.mock.calls as unknown as [string, RequestInit][]) {
      expect(new Headers(call[1].headers).get(PERSON_HEADER)).toBeNull();
    }
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("sends nothing when the shell has no key or fails, so the service answers desktop_required", async () => {
    for (const invoke of [vi.fn(async () => ""), vi.fn(async () => 7), vi.fn(async () => Promise.reject("unavailable"))]) {
      shell(invoke);
      expect(await personKeyHeaders("POST", "/api/approvals/a1/decide")).toEqual({});
    }
  });

  it("asks again after the service refuses a key, because it restarted", async () => {
    const invoke = vi.fn(async () => "first");
    shell(invoke);
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({ code: "person_required", message: "No." }), { status: 403 })));
    await expect(request("POST", "/api/approvals/a1/decide", {})).rejects.toMatchObject({ code: "person_required" });
    invoke.mockImplementation(async () => "second");
    const next = ok();
    vi.stubGlobal("fetch", next);
    await request("POST", "/api/approvals/a1/decide", {});
    expect(sentHeaders(next).get(PERSON_HEADER)).toBe("second");
  });

  it("makes one request for a write refused as person_required, names its ApiFailure, and asks the shell again for the next write", async () => {
    const invoke = vi.fn(async () => "first");
    shell(invoke);
    const refused = vi.fn(async () => new Response(JSON.stringify({ code: "person_required", message: "No." }), { status: 403 }));
    vi.stubGlobal("fetch", refused);
    const failure = await request("POST", "/api/crew/tasks/t1/cancel").catch((error: unknown) => error);
    expect(failure).toBeInstanceOf(ApiFailure);
    expect(failure).toMatchObject({ name: "ApiFailure", code: "person_required", status: 403 });
    expect(refused, "no automatic retry and no second send for the refused write").toHaveBeenCalledTimes(1);
    expect(String((refused.mock.calls[0] as unknown as [string, RequestInit])[0])).toBe("/api/crew/tasks/t1/cancel");

    invoke.mockImplementation(async () => "second");
    const next = ok();
    vi.stubGlobal("fetch", next);
    await request("POST", "/api/crew/tasks/t1/cancel");
    expect(sentHeaders(next).get(PERSON_HEADER)).toBe("second");
    expect(invoke, "the next write asks the shell's person_key again").toHaveBeenCalledTimes(2);
  });

  it("never puts the person key in a URL, storage, a console argument or an ApiFailure message", async () => {
    const key = "KEY-SENTINEL-123";
    shell(vi.fn(async () => key));
    localStorage.clear();
    sessionStorage.clear();
    const consoleSpies = (["log", "info", "warn", "error"] as const).map((level) => vi.spyOn(console, level).mockImplementation(() => undefined));

    const succeeded = ok();
    vi.stubGlobal("fetch", succeeded);
    await request("POST", "/api/crew/tasks/t1/cancel");
    expect(sentHeaders(succeeded).get(PERSON_HEADER)).toBe(key);

    const refused = vi.fn(async () => new Response(JSON.stringify({ code: "person_required", message: "No." }), { status: 403 }));
    vi.stubGlobal("fetch", refused);
    const failure = await request("POST", "/api/crew/tasks/t1/cancel").catch((error: unknown) => error);
    expect(failure).toBeInstanceOf(ApiFailure);
    expect(new Headers((refused.mock.calls[0] as unknown as [string, RequestInit])[1].headers).get(PERSON_HEADER)).toBe(key);

    for (const [url] of [...succeeded.mock.calls, ...refused.mock.calls] as unknown as [string, RequestInit][]) {
      expect(String(url)).not.toContain(key);
    }
    for (const storage of [localStorage, sessionStorage]) {
      for (let at = 0; at < storage.length; at += 1) {
        const name = storage.key(at) ?? "";
        expect(`${name}=${storage.getItem(name) ?? ""}`).not.toContain(key);
      }
    }
    for (const spy of consoleSpies) {
      for (const args of spy.mock.calls) expect(`${args.map((arg) => String(arg)).join(" ")} ${JSON.stringify(args)}`).not.toContain(key);
    }
    expect((failure as ApiFailure).message).not.toContain(key);
  });

  it("sends the fixed fake key where there is no shell, which only a dev or test build does", async () => {
    const fetchMock = ok();
    vi.stubGlobal("fetch", fetchMock);
    await request("POST", "/api/projects/p/worktrees/w/file-actions", { action: "create_file", path: "x" });
    expect(sentHeaders(fetchMock).get(PERSON_HEADER)).toBe("mock-person-key");
  });
});

it("download consent is a person-only update effect", () => {
  expect(sendsPersonKey("POST", "/api/updates/app/download")).toBe(true);
});

describe("the device actions of docs/devices.md", () => {
  it("send the key for every action type through one route, and not for the reads", async () => {
    expect(sendsPersonKey("POST", "/api/devices/android%3APixel_Fold/actions")).toBe(true);
    expect(sendsPersonKey("POST", "/api/devices/ios%3AUDID/actions")).toBe(true);
    expect(sendsPersonKey("GET", "/api/devices")).toBe(false);
    expect(sendsPersonKey("GET", "/api/devices/android%3Aa")).toBe(false);
    shell(vi.fn(async () => "shell-key"));
    const posted = ok();
    vi.stubGlobal("fetch", posted);
    await request("POST", "/api/devices/android%3Aa/actions", { type: "screenshot" });
    expect(sentHeaders(posted).get(PERSON_HEADER)).toBe("shell-key");
  });
});

describe("browser profile renames", () => {
  it("classifies only the POST rename route with a concrete profile ID", () => {
    expect(sendsPersonKey("POST", "/api/browser-profiles/profile-1/rename")).toBe(true);
    for (const [method, path] of [
      ["GET", "/api/browser-profiles/profile-1/rename"],
      ["POST", "/api/browser-profiles//rename"],
      ["POST", "/api/browser-profiles/profile-1/rename/extra"],
      ["GET", "/api/browser-profiles"],
      ["POST", "/api/browser-profiles"],
      ["DELETE", "/api/browser-profiles/profile-1"],
      ["POST", "/api/browser-profiles/profile-1/clear"],
    ] as const) {
      expect(sendsPersonKey(method, path), `${method} ${path}`).toBe(false);
    }
  });

  it("sends the shell key with the rename request", async () => {
    shell(vi.fn(async () => "shell-key"));
    const posted = ok();
    vi.stubGlobal("fetch", posted);
    await request("POST", "/api/browser-profiles/profile-1/rename", { name: "Research" });
    expect(sentHeaders(posted).get(PERSON_HEADER)).toBe("shell-key");
  });
});

describe("the profile writes of docs/execution-profiles-and-library.md", () => {
  it("send the key on every write, and on the reads and the check", () => {
    expect(sendsPersonKey("POST", "/api/profiles")).toBe(true);
    expect(sendsPersonKey("PATCH", "/api/profiles/01j0")).toBe(true);
    expect(sendsPersonKey("DELETE", "/api/profiles/01j0")).toBe(true);
    expect(sendsPersonKey("PUT", "/api/threads/t1/profile")).toBe(true);
    expect(sendsPersonKey("POST", "/api/crew/profile/apply")).toBe(true);
    for (const [method, path] of [["GET", "/api/profiles"], ["GET", "/api/profiles/01j0"], ["POST", "/api/profiles/check"]] as const) expect(sendsPersonKey(method, path), path).toBe(false);
  });
});

describe("which pages may send the key", () => {
  it("is true in the desktop app, whose shell has the bridge to ask", () => {
    shell(vi.fn(async () => "shell-key"));
    expect(canSendPersonKey()).toBe(true);
  });

  it("stays true in a dev preview, which sends the mock's fixed key, so the designed frames stay reachable there", () => {
    // No bridge: a browser tab. The preview build compiles `__ORBYN_PREVIEWS__` true (vite.config.ts), which is what the
    // prompt and the mock both rely on; a production build has it false and this answers false.
    expect(window as unknown as { __TAURI_INTERNALS__?: unknown }).not.toHaveProperty("__TAURI_INTERNALS__");
    expect(canSendPersonKey()).toBe(__ORBYN_PREVIEWS__);
  });

  it("sends no person key from a browser tab, where the page cannot send one", async () => {
    vi.stubGlobal("__ORBYN_PREVIEWS__", false);
    delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    expect(canSendPersonKey()).toBe(false);
    const sent = ok();
    vi.stubGlobal("fetch", sent);
    await request("POST", "/api/crew/tasks/t1/cancel");
    expect(sent).toHaveBeenCalledTimes(1);
    expect(sentHeaders(sent).get(PERSON_HEADER)).toBeNull();
  });
});
