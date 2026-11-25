import { afterEach, describe, expect, it, vi } from "vitest";
import { api } from "../../api/client";
import type { Approval, Project, Thread } from "../../api/types";
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
/**
 * Issue #1118's idle poll: one project list, one global thread list, and the approvals only while
 * there is a run they can belong to. Pending approvals are created by a waiting run and expired when
 * it finishes, and an open thread's stream reads one the moment it lands, so an idle app can leave
 * the list alone; the polled threads' `active_run` is what says whether one can exist.
 */
describe("idle approval polling", () => {
  const a = project("a");
  const active = (value: Thread): Thread => ({ ...value, active_run: { id: `run-${value.id}` } as never });

  it("reads one project list and one thread list, and no approvals, with no run to watch", async () => {
    useWorkspace.setState({ projects: ready([a]), threadIds: {}, threads: {}, approvals: ready([]) });
    const projectsRead = vi.spyOn(api, "projects").mockResolvedValue([a]);
    const all = vi.spyOn(api, "allThreads").mockResolvedValue([thread("one", "a")]);
    const perProject = vi.spyOn(api, "threads");
    const approvals = vi.spyOn(api, "pendingApprovals");
    await useWorkspace.getState().refreshAll({ quick: true });
    expect(projectsRead).toHaveBeenCalledTimes(1);
    expect(all).toHaveBeenCalledTimes(1);
    expect(perProject).not.toHaveBeenCalled();
    expect(approvals).not.toHaveBeenCalled();
  });

  it("reads approvals when the thread list fails, since a run elsewhere may be waiting", async () => {
    useWorkspace.setState({ projects: ready([a]), threadIds: {}, threads: {}, approvals: ready([]) });
    vi.spyOn(api, "projects").mockResolvedValue([a]);
    vi.spyOn(api, "allThreads").mockRejectedValue(new Error("unavailable"));
    vi.spyOn(api, "threads").mockResolvedValue([]);
    const approvals = vi.spyOn(api, "pendingApprovals").mockResolvedValue([{ id: "ap" } as Approval]);
    await useWorkspace.getState().refreshAll({ quick: true });
    expect(approvals).toHaveBeenCalledTimes(1);
    expect(useWorkspace.getState().approvals.data?.map((entry) => entry.id)).toEqual(["ap"]);
  });

  it("reads approvals while the project catalog fails, then idles again once it reads", async () => {
    useWorkspace.setState({ projects: ready([a]), threadIds: {}, threads: {}, approvals: ready([]) });
    const projectsRead = vi.spyOn(api, "projects").mockRejectedValue(new Error("unavailable"));
    vi.spyOn(api, "allThreads").mockResolvedValue([thread("one", "a")]);
    const approvals = vi.spyOn(api, "pendingApprovals").mockResolvedValue([{ id: "ap" } as Approval]);
    // The failed read may hide a project added since the last good one, so its waiting run is unseen.
    await useWorkspace.getState().refreshAll({ quick: true });
    expect(useWorkspace.getState().projects.error).not.toBeNull();
    expect(approvals).toHaveBeenCalledTimes(1);
    expect(useWorkspace.getState().approvals.data?.map((entry) => entry.id)).toEqual(["ap"]);
    // The catalog reads again and the pending clears, then the idle poll leaves approvals alone.
    projectsRead.mockResolvedValue([a]);
    vi.mocked(api.pendingApprovals).mockResolvedValue([]);
    await useWorkspace.getState().refreshAll({ quick: true });
    await useWorkspace.getState().refreshAll({ quick: true });
    expect(useWorkspace.getState().projects.error).toBeNull();
    expect(approvals).toHaveBeenCalledTimes(2);
  });

  it("keeps one approvals read a poll while a run known from the last poll can wait on one", async () => {
    const one = active(thread("one", "a"));
    useWorkspace.setState({ projects: ready([a]), threadIds: {}, threads: { one }, approvals: ready([]) });
    vi.spyOn(api, "projects").mockResolvedValue([a]);
    vi.spyOn(api, "allThreads").mockResolvedValue([one]);
    const approvals = vi.spyOn(api, "pendingApprovals").mockResolvedValue([{ id: "ap" } as Approval]);
    await useWorkspace.getState().refreshAll({ quick: true });
    expect(approvals).toHaveBeenCalledTimes(1);
    expect(useWorkspace.getState().approvals.data?.map((entry) => entry.id)).toEqual(["ap"]);
  });

  it("asks in the same poll that first sees a run that started since the last one", async () => {
    useWorkspace.setState({ projects: ready([a]), threadIds: {}, threads: {}, approvals: ready([]) });
    vi.spyOn(api, "projects").mockResolvedValue([a]);
    vi.spyOn(api, "allThreads").mockResolvedValue([active(thread("one", "a"))]);
    const approvals = vi.spyOn(api, "pendingApprovals").mockResolvedValue([{ id: "ap" } as Approval]);
    await useWorkspace.getState().refreshAll({ quick: true });
    expect(approvals).toHaveBeenCalledTimes(1);
    expect(useWorkspace.getState().approvals.data?.map((entry) => entry.id)).toEqual(["ap"]);
  });

  it("keeps reading one pending on screen so it clears when its run ends elsewhere", async () => {
    useWorkspace.setState({ projects: ready([a]), threadIds: {}, threads: {}, approvals: ready([{ id: "ap" } as Approval]) });
    vi.spyOn(api, "projects").mockResolvedValue([a]);
    vi.spyOn(api, "allThreads").mockResolvedValue([thread("one", "a")]);
    const approvals = vi.spyOn(api, "pendingApprovals").mockResolvedValue([]);
    await useWorkspace.getState().refreshAll({ quick: true });
    expect(approvals).toHaveBeenCalledTimes(1);
    expect(useWorkspace.getState().approvals.data).toEqual([]);
    // The list is empty and no run is listed now: the next idle poll leaves it alone.
    await useWorkspace.getState().refreshAll({ quick: true });
    expect(approvals).toHaveBeenCalledTimes(1);
  });
});
