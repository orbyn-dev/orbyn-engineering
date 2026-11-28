// The service's answer to "which Crew task made this pull request", and the two answers that mean no Run tab: no link, and
// a service that does not have the read. With Crew Preview off it asks nothing, because a Crew route would answer 404 and
// the browser logs that as an error.
import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiFailure } from "../../../api/client";
import { useConnection } from "../../../product/state/connection";
import { runLinkClient, setRunLinkClient } from "./link";

const answer = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });
const preview = (on: boolean) => useConnection.setState({ crewPreview: on });

afterEach(() => {
  setRunLinkClient(null);
  preview(false);
  vi.unstubAllGlobals();
});

describe("the service's run link", () => {
  it("names the task the service answered, asking its project and number", async () => {
    const fetch = vi.fn((_input: RequestInfo | URL, _init?: RequestInit) => Promise.resolve(answer({ task: "task-redirect" })));
    vi.stubGlobal("fetch", fetch);
    preview(true);
    await expect(runLinkClient().forPull("project one", 142)).resolves.toEqual({ task: "task-redirect" });
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(fetch.mock.calls[0]![0]).toBe("/api/crew/flow/run-link?project=project%20one&number=142");
  });

  it("answers no link for a 404, which is also an older service with no such read", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(answer({ code: "not_found", message: "no task made it" }, 404))));
    preview(true);
    await expect(runLinkClient().forPull("p1", 142)).resolves.toBeNull();
  });

  it("reports any other failure rather than hiding it", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(answer({ code: "internal", message: "the read broke" }, 500))));
    preview(true);
    const failure = await runLinkClient()
      .forPull("p1", 142)
      .catch((error: unknown) => error);
    expect(failure).toBeInstanceOf(ApiFailure);
    expect(failure).toMatchObject({ status: 500 });
  });

  it("asks nothing with Crew Preview off, where every Crew route is absent", async () => {
    const fetch = vi.fn(() => Promise.resolve(answer({ task: "task-redirect" })));
    vi.stubGlobal("fetch", fetch);
    preview(false);
    await expect(runLinkClient().forPull("p1", 142)).resolves.toBeNull();
    expect(fetch).not.toHaveBeenCalled();
  });

  it("puts the service's own answer back when a test clears its client", async () => {
    const forPull = vi.fn().mockResolvedValue({ task: "task-other" });
    setRunLinkClient({ forPull });
    await expect(runLinkClient().forPull("p1", 142)).resolves.toEqual({ task: "task-other" });
    setRunLinkClient(null);
    preview(false);
    await expect(runLinkClient().forPull("p1", 142)).resolves.toBeNull();
    expect(forPull).toHaveBeenCalledTimes(1);
  });
});
