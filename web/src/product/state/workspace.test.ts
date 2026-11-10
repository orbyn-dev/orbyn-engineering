import { describe, expect, it, vi } from "vitest";
import { api } from "../../api/client";
import * as projectImages from "../projects/projectImages";
import { projects } from "../../../mock/fixtures";
import type { Approval, Project, Thread } from "../../api/types";
import { useWorkspace } from "./workspace";

const approval = (id: string) => ({ id }) as Approval;

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => (resolve = done));
  return { promise, resolve };
}

describe("workspace store", () => {
  it.each(["icon", "name"] as const)("keeps a committed %s edit when an older project refresh arrives", async (field) => {
    const original: Project = { ...projects[0]!, name: "Before commit", metadata_revision: "4", project_icon: null };
    const peer: Project = { ...original, id: "peer", path: "/peer", name: "Peer" };
    const committed: Project = { ...original, metadata_revision: "5",
      ...(field === "icon" ? { project_icon: { kind: "emoji", emoji: "🚀" } as const } : { name: "Committed name" }) };
    useWorkspace.setState({ projects: { data: [original, peer], error: null, loading: false } });
    const stale = deferred<Project[]>();
    const spy = vi.spyOn(api, "projects").mockReturnValueOnce(stale.promise);
    try {
      const refresh = useWorkspace.getState().refreshProjects();
      useWorkspace.getState().putProject(committed);
      const loadingAtCommit = useWorkspace.getState().projects.loading;
      stale.resolve([original, peer]);
      await refresh;
      expect(useWorkspace.getState().projects).toEqual({ data: [committed, peer], error: null, loading: false });
      expect(loadingAtCommit).toBe(false);
    } finally { spy.mockRestore(); }
  });

  it("keeps a committed deletion when an older project refresh arrives", async () => {
    const removed: Project = { ...projects[0]!, metadata_revision: "4" };
    const peer: Project = { ...removed, id: "peer", path: "/peer", name: "Peer" };
    useWorkspace.setState({ projects: { data: [removed, peer], error: null, loading: false } });
    const stale = deferred<Project[]>();
    const spy = vi.spyOn(api, "projects").mockReturnValueOnce(stale.promise);
    try {
      const refresh = useWorkspace.getState().refreshProjects();
      useWorkspace.getState().dropProject(removed.id);
      const loadingAtCommit = useWorkspace.getState().projects.loading;
      stale.resolve([removed, peer]);
      await refresh;
      expect(useWorkspace.getState().projects).toEqual({ data: [peer], error: null, loading: false });
      expect(loadingAtCommit).toBe(false);
    } finally { spy.mockRestore(); }
  });

  it("ignores a stale response that arrives last", async () => {
    const first = deferred<Approval[]>();
    const second = deferred<Approval[]>();
    const spy = vi
      .spyOn(api, "pendingApprovals")
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);

    const a = useWorkspace.getState().refreshApprovals();
    const b = useWorkspace.getState().refreshApprovals();
    second.resolve([approval("new")]);
    await b;
    first.resolve([approval("old")]);
    await a;
    expect(useWorkspace.getState().approvals.data?.map((x) => x.id)).toEqual(["new"]);
    spy.mockRestore();
  });

  it("keeps the thread object when a single-thread refresh finds nothing new", async () => {
    const thread = { id: "t-same", title: "Same" } as Thread;
    useWorkspace.setState({ threads: { "t-same": thread }, missingThreads: {} });
    const before = useWorkspace.getState().threads;
    const spy = vi.spyOn(api, "thread").mockResolvedValue({ ...thread });
    await useWorkspace.getState().refreshThread("t-same");
    expect(useWorkspace.getState().threads).toBe(before);
    expect(useWorkspace.getState().threads["t-same"]).toBe(thread);
    spy.mockRestore();
  });

  it("replaces a changed thread and keeps its neighbours", async () => {
    const thread = { id: "t-two", title: "Before" } as Thread;
    const other = { id: "t-three", title: "Other" } as Thread;
    useWorkspace.setState({ threads: { "t-two": thread, "t-three": other }, missingThreads: {} });
    const before = useWorkspace.getState().threads;
    const spy = vi.spyOn(api, "thread").mockResolvedValue({ ...thread, title: "After" });
    await useWorkspace.getState().refreshThread("t-two");
    const after = useWorkspace.getState().threads;
    expect(after).not.toBe(before);
    expect(after["t-two"]?.title).toBe("After");
    expect(after["t-three"]).toBe(other);
    spy.mockRestore();
  });

  it("keeps the last data when a refresh fails", async () => {
    useWorkspace.setState({ approvals: { data: [approval("a")], error: null, loading: false } });
    const spy = vi.spyOn(api, "pendingApprovals").mockRejectedValueOnce(new Error("down"));
    await useWorkspace.getState().refreshApprovals();
    const { data, error } = useWorkspace.getState().approvals;
    expect(data?.map((x) => x.id)).toEqual(["a"]);
    expect(error).not.toBeNull();
    spy.mockRestore();
  });

  it("removes a decided approval from the pending list and keeps the receipt", () => {
    useWorkspace.setState({ approvals: { data: [approval("a")], error: null, loading: false } });
    useWorkspace.getState().putReceipt({ ...approval("a"), state: "allowed" });
    expect(useWorkspace.getState().approvals.data).toEqual([]);
    expect(useWorkspace.getState().receipts.a?.state).toBe("allowed");
  });
});


it("invalidates passive image observations when committed or polled icon metadata changes", async () => {
  const project = { ...projects[0]!, project_icon: null, favicon_path: null };
  useWorkspace.setState({ projects: { data: [project], error: null, loading: false } });
  const invalidateProjectImages = vi.spyOn(projectImages, "invalidateProjectImages");
  useWorkspace.getState().putProject({ ...project, name: "Renamed", metadata_revision: "1" });
  expect(invalidateProjectImages).not.toHaveBeenCalled();
  const icon = { kind: "emoji" as const, emoji: "🚀" };
  useWorkspace.getState().putProject({ ...project, project_icon: icon, metadata_revision: "2" });
  expect(invalidateProjectImages).toHaveBeenCalledExactlyOnceWith(project.id);
  vi.mocked(invalidateProjectImages).mockClear();
  const spy = vi.spyOn(api, "projects").mockResolvedValue([{ ...project, metadata_revision: "3" }]);
  try {
    await useWorkspace.getState().refreshProjects();
    expect(invalidateProjectImages).toHaveBeenCalledExactlyOnceWith(project.id);
    vi.mocked(invalidateProjectImages).mockClear();
    await useWorkspace.getState().refreshProjects();
    expect(invalidateProjectImages).not.toHaveBeenCalled();
  } finally { spy.mockRestore(); invalidateProjectImages.mockRestore(); }
});


it("does not replace a newer committed project with an older mutation response", () => {
  const older: Project = { ...projects[0]!, name: "Before rename", metadata_revision: "4", favicon_path: "brand/icon.svg" };
  const newer: Project = { ...older, name: "After rename", metadata_revision: "5" };
  useWorkspace.setState({ projects: { data: [older], error: null, loading: false } });
  useWorkspace.getState().putProject(newer);
  useWorkspace.getState().putProject(older);
  expect(useWorkspace.getState().projects.data).toEqual([newer]);
});


it("orders committed project revisions exactly above JavaScript's integer range", () => {
  const newer: Project = { ...projects[0]!, name: "Newer", metadata_revision: "9007199254740993" };
  useWorkspace.setState({ projects: { data: [newer], error: null, loading: false } });
  useWorkspace.getState().putProject({ ...newer, name: "Older", metadata_revision: "9007199254740992" });
  expect(useWorkspace.getState().projects.data).toEqual([newer]);
});

it("does not replace observed versioned project metadata with an unversioned reply", () => {
  const newer: Project = { ...projects[0]!, name: "Observed", metadata_revision: "0" };
  useWorkspace.setState({ projects: { data: [newer], error: null, loading: false } });
  useWorkspace.getState().putProject({ ...newer, name: "Unversioned", metadata_revision: undefined });
  expect(useWorkspace.getState().projects.data).toEqual([newer]);
});

describe("visible projects", () => {
  it("leaves the skills library out before the workspace status loads", async () => {
    const { renderHook } = await import("@testing-library/react");
    const { useVisibleProjects } = await import("./workspace");
    const library: Project = { ...projects[0]!, id: "library", path: "/library", name: "Skills library", library: true };
    const mine: Project = { ...projects[0]!, id: "mine", path: "/mine", name: "Mine", library: false };
    useWorkspace.setState({
      projects: { data: [library, mine], error: null, loading: false },
      managed: { data: null, error: null, loading: true },
    });
    const { result } = renderHook(() => useVisibleProjects());
    expect(result.current?.map((project) => project.id)).toEqual(["mine"]);
  });
});

describe("the quick refresh", () => {
  const stub = () => ({
    projects: vi.spyOn(api, "projects").mockResolvedValue([]),
    workspace: vi.spyOn(api, "workspace").mockResolvedValue({ configured: true, default_url: "", library: null, targets: [] } as never),
    updates: vi.spyOn(api, "updates").mockResolvedValue({ library: {}, app: {}, engines: [], checked_at: null, error: null } as never),
    approvals: vi.spyOn(api, "pendingApprovals").mockResolvedValue([]),
  });

  it("reads projects and approvals but not the Git-backed workspace and update status", async () => {
    const spies = stub();
    try {
      await useWorkspace.getState().refreshAll({ quick: true });
      expect(spies.projects).toHaveBeenCalled();
      expect(spies.approvals).toHaveBeenCalled();
      expect(spies.workspace).not.toHaveBeenCalled();
      expect(spies.updates).not.toHaveBeenCalled();
    } finally {
      Object.values(spies).forEach((spy) => spy.mockRestore());
    }
  });

  it("is still the full refresh when asked for no option", async () => {
    const spies = stub();
    try {
      await useWorkspace.getState().refreshAll();
      expect(spies.workspace).toHaveBeenCalledTimes(1);
      expect(spies.updates).toHaveBeenCalledTimes(1);
    } finally {
      Object.values(spies).forEach((spy) => spy.mockRestore());
    }
  });
});

describe("a run that ends", () => {
  it("reads the Git-backed workspace status again, since the agent may have committed or switched branch", async () => {
    const workspace = vi.spyOn(api, "workspace").mockResolvedValue({ configured: true, default_url: "", library: null, targets: [] } as never);
    try {
      const running = { id: "t1", active_run: { id: "r1" } } as never;
      useWorkspace.setState({ threads: { t1: running } });
      expect(workspace).not.toHaveBeenCalled();
      useWorkspace.setState({ threads: { t1: { id: "t1", active_run: null } as never } });
      await vi.waitFor(() => expect(workspace).toHaveBeenCalledTimes(1));
      // A thread that merely changes while idle does not.
      useWorkspace.setState({ threads: { t1: { id: "t1", active_run: null, title: "x" } as never } });
      expect(workspace).toHaveBeenCalledTimes(1);
    } finally {
      workspace.mockRestore();
    }
  });
});

describe("an unchanged poll", () => {
  const project: Project = { ...projects[0]!, metadata_revision: "4" };
  const thread = { id: "t1", project_id: project.id, title: "One", updated_at: "2026-11-10T00:00:00.000Z" } as unknown as Thread;
  const online = () => {
    useWorkspace.setState({
      projects: { data: [project], error: null, loading: false },
      threadIds: { [project.id]: { data: ["t1"], error: null, loading: false } },
      threads: { t1: thread },
      approvals: { data: [], error: null, loading: false },
    });
    const state = useWorkspace.getState();
    return { projects: state.projects, threads: state.threads, threadIds: state.threadIds, approvals: state.approvals };
  };
  const poll = (answer: { projects: Project[]; threads: Thread[]; approvals: Approval[] }) => [
    vi.spyOn(api, "projects").mockResolvedValue(answer.projects),
    vi.spyOn(api, "threads").mockResolvedValue(answer.threads),
    vi.spyOn(api, "allThreads").mockResolvedValue([]),
    vi.spyOn(api, "pendingApprovals").mockResolvedValue(answer.approvals),
  ];

  it("leaves every object the sidebar reads at the same identity", async () => {
    const before = online();
    const spies = poll({ projects: [{ ...project }], threads: [{ ...thread }], approvals: [] });
    try {
      await useWorkspace.getState().refreshAll({ quick: true });
      const after = useWorkspace.getState();
      expect(after.projects).toBe(before.projects);
      expect(after.threads).toBe(before.threads);
      expect(after.threadIds).toBe(before.threadIds);
      expect(after.approvals).toBe(before.approvals);
    } finally {
      spies.forEach((spy) => spy.mockRestore());
    }
  });

  it("replaces the thread whose content changed and keeps the other", async () => {
    online();
    const second = { ...thread, id: "t2", title: "Two" };
    useWorkspace.setState((state) => ({ threads: { ...state.threads, t2: second } }));
    const before = useWorkspace.getState().threads;
    const spies = poll({ projects: [{ ...project }], threads: [{ ...thread, title: "Renamed" }, { ...second }], approvals: [] });
    try {
      await useWorkspace.getState().refreshAll({ quick: true });
      const after = useWorkspace.getState().threads;
      expect(after).not.toBe(before);
      expect(after.t1?.title).toBe("Renamed");
      expect(after.t2).toBe(second);
    } finally {
      spies.forEach((spy) => spy.mockRestore());
    }
  });

  it("shows loading only while there is nothing on screen", async () => {
    useWorkspace.setState({ projects: { data: null, error: null, loading: false } });
    const first = deferred<Project[]>();
    const spy = vi.spyOn(api, "projects").mockReturnValueOnce(first.promise);
    try {
      const refresh = useWorkspace.getState().refreshProjects();
      expect(useWorkspace.getState().projects.loading).toBe(true);
      first.resolve([project]);
      await refresh;
      const second = deferred<Project[]>();
      spy.mockReturnValueOnce(second.promise);
      const again = useWorkspace.getState().refreshProjects();
      expect(useWorkspace.getState().projects.loading).toBe(false);
      second.resolve([{ ...project }]);
      await again;
      expect(useWorkspace.getState().projects).toEqual({ data: [project], error: null, loading: false });
    } finally {
      spy.mockRestore();
    }
  });
});
