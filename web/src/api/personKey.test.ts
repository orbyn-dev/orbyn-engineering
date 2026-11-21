import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { request } from "./client";
import { forgetPersonKey, PENDING_PERSON_ROUTES, PERSON_HEADER, PERSON_ROUTES, personKeyHeaders, personKeyProtocols, sendsPersonKey } from "./personKey";

const ok = () => vi.fn(async () => new Response(null, { status: 204 }));
const sentHeaders = (fetchMock: ReturnType<typeof ok>) => new Headers((fetchMock.mock.calls[0] as unknown as [string, RequestInit])[1].headers);

function shell(invoke: (command: string) => unknown) {
  (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke };
}

afterEach(() => {
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

describe("the Cursor thread routes, which are not on the service yet", () => {
  it("send the key on the override and the acknowledgement, and on no read of the notice", () => {
    expect(sendsPersonKey("POST", "/api/threads/t1/send-without-approvals")).toBe(true);
    expect(sendsPersonKey("POST", "/api/engines/cursor/notice/acknowledge")).toBe(true);
    expect(sendsPersonKey("GET", "/api/engines/cursor/notice")).toBe(false);
    expect(sendsPersonKey("GET", "/api/threads/t1/send-without-approvals")).toBe(false);
    expect(sendsPersonKey("POST", "/api/engines/cursor/notice")).toBe(false);
  });

  it("stay out of PERSON_ROUTES until routes.json lists them, and leave once it does", () => {
    const table = JSON.parse(readFileSync(resolve(import.meta.dirname, "../../../crates/orbynd/tests/routes.json"), "utf8")) as {
      protected: [string, string, string][];
    };
    for (const [method, pattern] of PENDING_PERSON_ROUTES) {
      const landed = table.protected.some(([m, p]) => m === method && p === pattern);
      expect(landed, `${method} ${pattern} is in routes.json now: move it into PERSON_ROUTES and delete it from PENDING_PERSON_ROUTES`).toBe(false);
      expect(PERSON_ROUTES.some(([m, p]) => m === method && p === pattern)).toBe(false);
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
    await request("POST", "/api/engines/cursor/notice/acknowledge");
    expect(sentHeaders(acknowledged).get(PERSON_HEADER)).toBe("shell-key");
  });
});

describe("the multi-repository writes of docs/multi-repo.md", () => {
  const writes = [
    "/api/projects/p1/repos/add-folder", "/api/projects/p1/repos/create", "/api/projects/p1/repos/clone", "/api/projects/p1/repos/r1/publish",
    "/api/projects/p1/repos/r1/trust", "/api/projects/p1/repos/r1/setup-git", "/api/projects/p1/repos/r1/remove", "/api/projects/p1/repos/r1/clone/cancel",
    "/api/projects/p1/repo-threads", "/api/threads/t1/repos",
  ];
  it("send the key on every write and on no read", () => {
    for (const path of writes) expect(sendsPersonKey("POST", path), path).toBe(true);
    expect(sendsPersonKey("GET", "/api/projects/p1/repos")).toBe(false);
    expect(sendsPersonKey("GET", "/api/threads/t1/repos")).toBe(false);
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
