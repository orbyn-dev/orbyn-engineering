import { renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiFailure } from "../../../../api/client";
import { runPage } from "../../runpage/fixture";
import { readRun } from "./api";
import { useRunPage } from "./useFlow";

const answer = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });

afterEach(() => vi.unstubAllGlobals());

describe("readRun", () => {
  it("reads the task's run on its own route with the id encoded", async () => {
    const fetch = vi.fn((_input: RequestInfo | URL, _init?: RequestInit) => Promise.resolve(answer(runPage())));
    vi.stubGlobal("fetch", fetch);
    const run = await readRun("task/one two");
    expect(run.task).toBe("task-redirect");
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(fetch.mock.calls[0]![0]).toBe("/api/crew/flow/tasks/task%2Fone%20two/run");
  });

  it("rejects a 404 as an ApiFailure so the page can fall back to the Flow read", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(answer({ code: "not_found", message: "task not found" }, 404))));
    await expect(readRun("task-redirect")).rejects.toMatchObject({ status: 404, code: "not_found" });
  });

  it("rejects any other failure with the service's error", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(answer({ code: "internal", message: "the run could not be read" }, 500))));
    const failure = await readRun("task-redirect").catch((error: unknown) => error);
    expect(failure).toBeInstanceOf(ApiFailure);
    expect(failure).toMatchObject({ status: 500, code: "internal" });
  });
});

describe("useRunPage", () => {
  it("hands the hook the task's run read", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(answer(runPage()))));
    const { result } = renderHook(() => useRunPage("task-redirect"));
    await waitFor(() => expect(result.current.status).toBe("ready"));
    expect(result.current.data?.task).toBe("task-redirect");
    expect(result.current.data?.checks[0]?.label).toBe("pnpm lint");
  });

  it("keeps a 404 as the hook's error for the page to treat as no run read", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(answer({ code: "not_found", message: "task not found" }, 404))));
    const { result } = renderHook(() => useRunPage("task-redirect"));
    await waitFor(() => expect(result.current.status).toBe("error"));
    expect(result.current.data).toBeNull();
    expect(result.current.error).toMatchObject({ status: 404 });
  });
});
