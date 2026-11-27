import { afterEach, describe, expect, it, vi } from "vitest";
import { api } from "../../api/client";
import type { Project, Thread } from "../../api/types";
import { projects } from "../../../mock/fixtures";
import { useWorkspace } from "./workspace";

const project = (id: string): Project => ({ ...projects[0]!, id });
const thread = (id: string, projectId: string, title = id, scratch = false): Thread =>
  ({ id, project_id: projectId, title, scratch } as Thread);
const ready = <T,>(data: T) => ({ data, error: null, loading: false });

afterEach(() => vi.restoreAllMocks());

describe("consolidated thread polling", () => {
  it("uses one list, preserving project order, empty projects, scratch and stable references", async () => {
    const a = project("a"), b = project("b"), empty = project("empty");
    const one = thread("one", "a"), two = thread("two", "a"), other = thread("other", "b");
    const scratch = thread("scratch", "hidden", "scratch", true);
    useWorkspace.setState({ projects: ready([a, b, empty]), threadIds: {}, threads: {}, approvals: ready([]) });
    vi.spyOn(api, "projects").mockResolvedValue([a, b, empty]);
    vi.spyOn(api, "pendingApprovals").mockResolvedValue([]);
    const all = vi.spyOn(api, "allThreads").mockResolvedValue([two, other, scratch, one]);
    const perProject = vi.spyOn(api, "threads");
    await useWorkspace.getState().refreshAll({ quick: true });
    let state = useWorkspace.getState();
    expect(all).toHaveBeenCalledTimes(1);
    expect(perProject).not.toHaveBeenCalled();
    expect(state.threadIds.a?.data).toEqual(["two", "one"]);
    expect(state.threadIds.b?.data).toEqual(["other"]);
    expect(state.threadIds.empty?.data).toEqual([]);
    expect(state.threads.scratch).toEqual(scratch);
    const before = { threads: state.threads, ids: state.threadIds };
    await useWorkspace.getState().refreshAll({ quick: true });
    state = useWorkspace.getState();
    expect(state.threads).toBe(before.threads);
    expect(state.threadIds).toBe(before.ids);
  });

  it("falls back to independent project reads on global failure, preserving project errors and scratch", async () => {
    const a = project("a"), b = project("b");
    const old = thread("old", "b"), scratch = thread("scratch", "hidden", "scratch", true);
    useWorkspace.setState({ projects: ready([a, b]), threadIds: { b: ready(["old"]) }, threads: { old, scratch }, approvals: ready([]) });
    vi.spyOn(api, "projects").mockResolvedValue([a, b]);
    vi.spyOn(api, "pendingApprovals").mockResolvedValue([]);
    vi.spyOn(api, "allThreads").mockRejectedValue(new Error("unavailable"));
    const perProject = vi.spyOn(api, "threads").mockImplementation((id) => id === "a"
      ? Promise.resolve([thread("fresh", "a")]) : Promise.reject(new Error("failed")));
    await useWorkspace.getState().refreshAll({ quick: true });
    const state = useWorkspace.getState();
    expect(perProject).toHaveBeenCalledTimes(2);
    expect(state.threadIds.a?.data).toEqual(["fresh"]);
    expect(state.threadIds.b?.data).toEqual(["old"]);
    expect(state.threadIds.b?.error).not.toBeNull();
    expect(state.threads.scratch).toBe(scratch);
  });

  it("does not replace a newer project refresh with a late global result", async () => {
    const a = project("a"), b = project("b");
    useWorkspace.setState({ projects: ready([a, b]), threadIds: {}, threads: {}, approvals: ready([]) });
    vi.spyOn(api, "projects").mockResolvedValue([a, b]);
    vi.spyOn(api, "pendingApprovals").mockResolvedValue([]);
    let resolve!: (threads: Thread[]) => void;
    const waiting = new Promise<Thread[]>((done) => { resolve = done; });
    const all = vi.spyOn(api, "allThreads").mockReturnValue(waiting);
    vi.spyOn(api, "threads").mockResolvedValue([thread("new", "a")]);
    const poll = useWorkspace.getState().refreshAll({ quick: true });
    await vi.waitFor(() => expect(all).toHaveBeenCalledTimes(1));
    await useWorkspace.getState().refreshThreads("a");
    resolve([thread("old", "a"), thread("other", "b")]);
    await poll;
    expect(useWorkspace.getState().threadIds.a?.data).toEqual(["new"]);
    expect(useWorkspace.getState().threads.old).toBeUndefined();
    expect(useWorkspace.getState().threadIds.b?.data).toEqual(["other"]);
  });
});
