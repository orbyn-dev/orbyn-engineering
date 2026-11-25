import { renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiFailure } from "../../../../api/client";
import { readTaskArtifacts } from "./api";
import { taskArtifacts } from "./fixture";
import { useTaskArtifacts } from "./useFlow";

const answer = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });

afterEach(() => vi.unstubAllGlobals());

describe("readTaskArtifacts", () => {
  it("reads the task's checks, artifacts and diff on their own route with the id encoded", async () => {
    const fetch = vi.fn((_input: RequestInfo | URL, _init?: RequestInit) => Promise.resolve(answer(taskArtifacts())));
    vi.stubGlobal("fetch", fetch);
    const read = await readTaskArtifacts("task/one two");
    // The shape the view draws: the checks, the artifacts each row named and the diff pinned to its base commit.
    expect(read.task).toBe("task-redirect");
    expect(read.checks[0]?.id).toBe("check-1");
    expect(read.checks[0]?.kind).toBe("test");
    expect(read.checks[0]?.outcome).toBe("pass");
    expect(read.checks[0]?.commit).toBe("a41c9e2c0ffee0123456789abcdef0123456789a");
    expect(read.checks[0]?.artifacts[0]).toEqual({ kind: "log", name: "pnpm-test-auth.log", hash: "sha256:7e1c4b94a" });
    expect(read.diff?.base).toBe("0b1e5f00c0ffee0123456789abcdef0123456789b");
    expect(read.diff?.files_total).toBe(2);
    expect(read.diff?.added).toBe(18);
    expect(read.diff?.removed).toBe(4);
    expect(read.diff_absent).toBeNull();
    expect(read.truncated).toBe(false);
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(fetch.mock.calls[0]![0]).toBe("/api/crew/flow/tasks/task%2Fone%20two/artifacts");
  });

  it("keeps the read's own reasons for a missing diff and the cut list", async () => {
    const promised = taskArtifacts({ checks: [], diff: null, diff_absent: "unreadable", truncated: true });
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(answer(promised))));
    const read = await readTaskArtifacts("task-redirect");
    expect(read.checks).toEqual([]);
    expect(read.diff).toBeNull();
    expect(read.diff_absent).toBe("unreadable");
    expect(read.truncated).toBe(true);
  });

  it("rejects a 404 as an ApiFailure so the view can say the read is not served", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(answer({ code: "not_found", message: "task not found" }, 404))));
    await expect(readTaskArtifacts("task-redirect")).rejects.toMatchObject({ status: 404, code: "not_found" });
  });

  it("rejects any other failure with the service's error", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(answer({ code: "internal", message: "the artifacts could not be read" }, 500))));
    const failure = await readTaskArtifacts("task-redirect").catch((error: unknown) => error);
    expect(failure).toBeInstanceOf(ApiFailure);
    expect(failure).toMatchObject({ status: 500, code: "internal" });
  });
});

describe("useTaskArtifacts", () => {
  it("hands the view the task's artifacts read", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(answer(taskArtifacts()))));
    const { result } = renderHook(() => useTaskArtifacts("task-redirect"));
    await waitFor(() => expect(result.current.status).toBe("ready"));
    expect(result.current.data?.task).toBe("task-redirect");
    expect(result.current.data?.checks[1]?.outcome).toBe("fail");
  });

  it("keeps a 404 as the hook's error for the view to say the read is not served", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(answer({ code: "not_found", message: "not found" }, 404))));
    const { result } = renderHook(() => useTaskArtifacts("task-redirect"));
    await waitFor(() => expect(result.current.status).toBe("error"));
    expect(result.current.data).toBeNull();
    expect(result.current.error).toMatchObject({ status: 404 });
  });
});
