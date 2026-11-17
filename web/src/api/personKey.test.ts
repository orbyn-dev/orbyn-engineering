import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { request } from "./client";
import { forgetPersonKey, PERSON_HEADER, PERSON_ROUTES, personKeyHeaders, sendsPersonKey } from "./personKey";

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
  it("names the two enforced routes and no other", () => {
    expect(sendsPersonKey("POST", "/api/approvals/a1/decide")).toBe(true);
    expect(sendsPersonKey("POST", "/api/projects/p/worktrees/w/file-actions")).toBe(true);
    expect(sendsPersonKey("post", "/api/approvals/a1/decide?x=1")).toBe(true);
    for (const [method, path] of [
      ["GET", "/api/approvals/a1/decide"],
      ["GET", "/api/approvals?state=pending"],
      ["POST", "/api/approvals/a1/decide/extra"],
      ["POST", "/api/approvals//decide"],
      ["POST", "/api/projects/p/worktrees/w/file"],
      ["POST", "/api/threads/t1/console"],
      ["PATCH", "/api/threads/t1"],
    ] as const) {
      expect(sendsPersonKey(method, path), `${method} ${path}`).toBe(false);
    }
  });

  it("matches the service's route table: every entry is a person route there", () => {
    const table = JSON.parse(readFileSync(resolve(import.meta.dirname, "../../../crates/orbynd/tests/routes.json"), "utf8")) as {
      protected: [string, string, string][];
    };
    for (const [method, , pattern] of PERSON_ROUTES) {
      expect(table.protected.some(([m, p, kind]) => m === method && p === pattern && kind === "person"), `${method} ${pattern}`).toBe(true);
    }
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
