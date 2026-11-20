import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiFailure, NETWORK_ERROR } from "./client";
import { forgetPersonKey, PERSON_HEADER } from "./personKey";
import { rpcWritesApi } from "./rpcWrites";

type Call = [string, RequestInit];

function stub(response: () => Response) {
  const fetchMock = vi.fn(async () => response());
  vi.stubGlobal("fetch", fetchMock);
  const call = () => fetchMock.mock.calls[0] as unknown as Call;
  return {
    fetchMock,
    url: () => call()[0],
    method: () => call()[1].method,
    headers: () => new Headers(call()[1].headers),
    body: () => JSON.parse(call()[1].body as string) as unknown,
  };
}

const noContent = () => new Response(null, { status: 204 });
const refused = (status: number, code: string) => () => new Response(JSON.stringify({ code, message: "No." }), { status });

afterEach(() => {
  vi.unstubAllGlobals();
  delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  forgetPersonKey();
});

describe("rpcWritesApi.mutateFile (Person)", () => {
  it("posts each action as the service's tagged body, with the person key", async () => {
    for (const action of [
      { action: "create_file", path: "a.txt" },
      { action: "create_directory", path: "dir" },
      { action: "move", path: "a.txt", destination: "b.txt" },
      { action: "delete", path: "b.txt" },
    ] as const) {
      const sent = stub(noContent);
      await expect(rpcWritesApi.mutateFile("t 1", action)).resolves.toBeUndefined();
      expect(sent.method()).toBe("POST");
      expect(sent.url()).toContain("/api/threads/t%201/files/mutate");
      expect(sent.body()).toEqual(action);
      expect(sent.headers().get(PERSON_HEADER)).toBe("mock-person-key");
    }
  });

  it("maps a refusal to ApiFailure with the service's code and status", async () => {
    stub(refused(403, "desktop_required"));
    await expect(rpcWritesApi.mutateFile("t1", { action: "delete", path: "x" })).rejects.toMatchObject({
      name: "ApiFailure",
      code: "desktop_required",
      status: 403,
    });
    stub(refused(400, "invalid_path"));
    await expect(rpcWritesApi.mutateFile("t1", { action: "delete", path: ".git/config" })).rejects.toMatchObject({ code: "invalid_path", status: 400 });
  });

  it("forgets a key the service refused, so the next call asks the shell again", async () => {
    const invoke = vi.fn(async () => "first");
    (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke };
    stub(refused(403, "person_required"));
    await expect(rpcWritesApi.mutateFile("t1", { action: "create_file", path: "x" })).rejects.toMatchObject({ code: "person_required" });
    invoke.mockImplementation(async () => "second");
    const next = stub(noContent);
    await rpcWritesApi.mutateFile("t1", { action: "create_file", path: "x" });
    expect(next.headers().get(PERSON_HEADER)).toBe("second");
  });

  it("sends no key where the shell has none", async () => {
    (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = { invoke: vi.fn(async () => "") };
    const sent = stub(noContent);
    await rpcWritesApi.mutateFile("t1", { action: "create_file", path: "x" });
    expect(sent.headers().get(PERSON_HEADER)).toBeNull();
  });

  it("maps an unreachable service to network_error", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => Promise.reject(new TypeError("down"))));
    const failure = await rpcWritesApi.mutateFile("t1", { action: "delete", path: "x" }).catch((error: unknown) => error);
    expect(failure).toBeInstanceOf(ApiFailure);
    expect(failure).toMatchObject({ code: NETWORK_ERROR, status: 0 });
  });
});

describe("rpcWritesApi.removeWorktree (Person)", () => {
  it("deletes the encoded worktree id with the person key", async () => {
    const sent = stub(noContent);
    await expect(rpcWritesApi.removeWorktree("p/1", "wt-0123456789abcdef")).resolves.toBeUndefined();
    expect(sent.method()).toBe("DELETE");
    expect(sent.url()).toContain("/api/projects/p%2F1/worktrees/wt-0123456789abcdef");
    expect(sent.fetchMock.mock.calls[0]).toBeDefined();
    expect((sent.fetchMock.mock.calls[0] as unknown as Call)[1].body).toBeUndefined();
    expect(sent.headers().get(PERSON_HEADER)).toBe("mock-person-key");
  });

  it("maps refusals (primary checkout, in use, token only) to ApiFailure", async () => {
    for (const [status, code] of [[403, "desktop_required"], [409, "conflict"], [404, "not_found"]] as const) {
      stub(refused(status, code));
      await expect(rpcWritesApi.removeWorktree("p", "wt-0000000000000000")).rejects.toMatchObject({ code, status });
    }
  });
});

describe("rpcWritesApi.refreshVcsStatus (Local)", () => {
  const status = { branch: "main", detached: false, head: "abc1234", upstream: "origin/main", ahead: 1, behind: 0, changed: 2, untracked: 1, conflicted: 0 };

  it("posts with no body and no person key, and returns the VcsStatus", async () => {
    const sent = stub(() => new Response(JSON.stringify(status), { status: 200 }));
    await expect(rpcWritesApi.refreshVcsStatus("t1")).resolves.toEqual(status);
    expect(sent.method()).toBe("POST");
    expect(sent.url()).toContain("/api/threads/t1/vcs/refresh-status");
    expect((sent.fetchMock.mock.calls[0] as unknown as Call)[1].body).toBeUndefined();
    expect(sent.headers().get(PERSON_HEADER)).toBeNull();
  });

  it("maps a folder outside Git to ApiFailure", async () => {
    stub(refused(409, "not_a_repository"));
    await expect(rpcWritesApi.refreshVcsStatus("t1")).rejects.toMatchObject({ code: "not_a_repository", status: 409 });
  });
});

describe("rpcWritesApi.createRef and switchRef (Local)", () => {
  it("creates a ref with an optional start and no person key", async () => {
    const withStart = stub(() => new Response(null, { status: 201 }));
    await expect(rpcWritesApi.createRef("t1", "topic", "main")).resolves.toBeUndefined();
    expect(withStart.method()).toBe("POST");
    expect(withStart.url()).toContain("/api/threads/t1/vcs/refs");
    expect(withStart.url()).not.toContain("switch");
    expect(withStart.body()).toEqual({ name: "topic", start: "main" });
    expect(withStart.headers().get(PERSON_HEADER)).toBeNull();

    const without = stub(() => new Response(null, { status: 201 }));
    await rpcWritesApi.createRef("t1", "topic");
    expect(without.body()).toEqual({ name: "topic" });
  });

  it("switches to an existing ref", async () => {
    const sent = stub(noContent);
    await expect(rpcWritesApi.switchRef("t1", "topic")).resolves.toBeUndefined();
    expect(sent.method()).toBe("POST");
    expect(sent.url()).toContain("/api/threads/t1/vcs/refs/switch");
    expect(sent.body()).toEqual({ name: "topic" });
    expect(sent.headers().get(PERSON_HEADER)).toBeNull();
  });

  it("maps a refused name or a missing ref to ApiFailure", async () => {
    stub(refused(400, "invalid_name"));
    await expect(rpcWritesApi.createRef("t1", "x", "-bad")).rejects.toMatchObject({ code: "invalid_name", status: 400 });
    stub(refused(404, "not_found"));
    await expect(rpcWritesApi.switchRef("t1", "missing")).rejects.toMatchObject({ code: "not_found", status: 404 });
  });
});
