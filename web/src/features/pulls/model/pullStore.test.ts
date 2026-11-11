import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { pullsApi } from "../api";
import type { PullRequest } from "../../../api/types";
import { reloadHeldPulls, useThreadPulls } from "./pullStore";

const pull = (number: number, over: Partial<PullRequest> = {}) =>
  ({ number, state: "open", draft: false, checks_state: "pass", title: `Pull ${number}`, ...over }) as unknown as PullRequest;

const thread = { id: "t1", project_id: "p" };

beforeEach(() => {
  vi.spyOn(pullsApi, "watches").mockResolvedValue([]);
});
afterEach(() => {
  vi.restoreAllMocks();
  useThreadPulls.setState({ byThread: {} });
});

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("pull requests linked to a thread", () => {
  it("reads once per thread for a burst of topics, and reads mergeable states again", async () => {
    const linked = vi.spyOn(pullsApi, "linked").mockResolvedValue([pull(1)]);
    const detail = vi.spyOn(pullsApi, "detail").mockResolvedValue({ mergeable: "mergeable" } as never);
    const release = useThreadPulls.getState().retain(thread.id);
    await useThreadPulls.getState().load(thread);
    linked.mockClear();
    detail.mockClear();
    reloadHeldPulls(["pulls", "pull:p:1", "pull:p:2"]);
    await settle();
    expect(linked).toHaveBeenCalledTimes(1);
    // A quiet read still refreshes the mergeable label, which a change to the pull request may have moved.
    expect(detail).toHaveBeenCalledTimes(1);
    release();
  });

  it("ignores a topic of another project, including one whose id starts with this project's", async () => {
    const linked = vi.spyOn(pullsApi, "linked").mockResolvedValue([]);
    vi.spyOn(pullsApi, "detail").mockResolvedValue({ mergeable: "mergeable" } as never);
    const release = useThreadPulls.getState().retain(thread.id);
    await useThreadPulls.getState().load({ id: "t1", project_id: "a" });
    linked.mockClear();
    reloadHeldPulls(["pull:a:b:7"]);
    await settle();
    expect(linked).not.toHaveBeenCalled();
    release();
  });

  it("does not let a quiet read discard the answer of a refresh the person asked for", async () => {
    let finishRefresh!: (list: PullRequest[]) => void;
    vi.spyOn(pullsApi, "linked").mockResolvedValue([pull(1, { title: "quiet" })]);
    vi.spyOn(pullsApi, "refreshLinked").mockReturnValue(new Promise<PullRequest[]>((resolve) => (finishRefresh = resolve)));
    vi.spyOn(pullsApi, "detail").mockResolvedValue({ mergeable: "mergeable" } as never);
    const release = useThreadPulls.getState().retain(thread.id);
    await useThreadPulls.getState().load(thread);
    const refresh = useThreadPulls.getState().refresh(thread);
    reloadHeldPulls(["pulls"]);
    await settle();
    finishRefresh([pull(1, { title: "forced" })]);
    await refresh;
    await settle();
    // The forced answer landed; the quiet read waited for it and then ran, so the list is the newest.
    expect(useThreadPulls.getState().byThread.t1?.pulls?.[0]?.title).toBe("quiet");
    expect(useThreadPulls.getState().byThread.t1?.busy).toBe(false);
    release();
  });

  it("forgets the thread's project when it is dropped", async () => {
    const linked = vi.spyOn(pullsApi, "linked").mockResolvedValue([]);
    const release = useThreadPulls.getState().retain(thread.id);
    await useThreadPulls.getState().load(thread);
    useThreadPulls.getState().forget(thread.id);
    linked.mockClear();
    reloadHeldPulls(["pulls"]);
    await settle();
    expect(linked).not.toHaveBeenCalled();
    release();
  });
});
