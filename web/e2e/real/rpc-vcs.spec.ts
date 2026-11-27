import { expect, test } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { startProject, type Service } from "./service";

/**
 * R5 VCS and git, through the real service routes: branches list/create/switch, the explicit
 * refresh-status, refs create and switch, commits, the worktree diff, one git/action commit, a
 * project pull (with an upstream), git init on an added folder, a sibling worktree through thread
 * admission and removal of a kept worktree through DELETE /api/projects/{id}/worktrees/{worktree}
 * (including the refusal while it holds a change).
 */
test.describe.configure({ mode: "serial" });

let service: Service;
let projectId = "";
let engineId = "";
let threadId = "";

interface BranchInfo { name: string; current: boolean; remote: boolean }
interface VcsStatus { branch: string | null; head: string; upstream: string | null; changed: number; untracked: number }
interface FileDiff { path: string; status: string; additions: number; deletions: number }
interface GitActionResult { phases: { name: string; ok: boolean; message: string | null }[] }
interface Worktrees { project_id: string; git: boolean; worktrees: { id: string; path: string; name: string; branch: string | null; is_main: boolean }[] }

const api = <T,>(method: string, path: string, body?: unknown) => service.api<T>(method, path, body);
const token = () => new URL(service.page).searchParams.get("token") ?? "";
async function status(method: string, path: string, body?: unknown): Promise<number> {
  const response = await fetch(`${service.url}${path}`, { method, headers: { Authorization: `Bearer ${token()}`, ...(body === undefined ? {} : { "Content-Type": "application/json" }) }, body: body === undefined ? undefined : JSON.stringify(body) });
  return response.status;
}
const createThread = (body: Record<string, unknown>) => api<{ id: string }>("POST", "/api/threads", { model: null, title: "RPC vcs", worktree: null, skill_profile: null, options: {}, ...body });

test.beforeAll(async () => {
  const started = await startProject({ fakeEngines: true, forge: true });
  service = started.service;
  projectId = started.projectId;
  engineId = started.engines[0]!.id;
  // The fixture's remote is a bare origin; give main an upstream so the project pull route has one.
  execFileSync("git", ["branch", "--set-upstream-to=origin/main", "main"], { cwd: service.repo, stdio: "ignore" });
  threadId = (await createThread({ project_id: projectId, engine: engineId })).id;
});
test.afterAll(() => service.stop());

test("branches, refs, refresh-status, the diff, one commit and commits read through the routes", async () => {
  const start = await api<BranchInfo[]>("GET", `/api/threads/${threadId}/branches`);
  expect(start.some((branch) => branch.name === "main" && branch.current)).toBe(true);

  expect(await status("POST", `/api/threads/${threadId}/branches`, { name: "rpc-one" })).toBe(201);
  const created = await api<BranchInfo[]>("GET", `/api/threads/${threadId}/branches`);
  const names = created.map((branch) => `${branch.name}${branch.current ? "*" : ""}`).join(", ");
  expect(names).toContain("rpc-one");
  expect(await status("POST", `/api/threads/${threadId}/branches/switch`, { name: "rpc-one" })).toBe(204);
  const refreshed = await api<VcsStatus>("POST", `/api/threads/${threadId}/vcs/refresh-status`);
  expect(refreshed.branch).toBe("rpc-one");
  expect(refreshed.head).not.toBe("");

  await api("PUT", `/api/threads/${threadId}/files/content`, { path: "rpc-change.txt", content: "rpc\n", base_revision: null });
  const diff = await api<FileDiff[]>("GET", `/api/threads/${threadId}/diff?scope=worktree`);
  expect(diff.some((file) => file.path === "rpc-change.txt")).toBe(true);

  const action = await api<GitActionResult>("POST", `/api/threads/${threadId}/git/action`, { commit_message: "rpc commit", push: false });
  expect(action.phases.length).toBeGreaterThan(0);
  expect(action.phases.at(-1)).toMatchObject({ name: "commit", ok: true });
  expect(JSON.stringify(await api<unknown>("GET", `/api/threads/${threadId}/commits`))).toContain("rpc commit");

  expect(await status("POST", `/api/threads/${threadId}/vcs/refs`, { name: "rpc-ref", start: "main" })).toBe(201);
  expect(await status("POST", `/api/threads/${threadId}/vcs/refs/switch`, { name: "rpc-ref" })).toBe(204);
  const switched = await api<VcsStatus>("POST", `/api/threads/${threadId}/vcs/refresh-status`);
  expect(switched.branch).toBe("rpc-ref");
});

test("the project pull, git init and the worktree list and removal routes answer", async () => {
  // Pull runs on the branch with an upstream (main); the earlier test left the checkout on rpc-ref.
  expect(await status("POST", `/api/threads/${threadId}/branches/switch`, { name: "main" })).toBe(204);
  expect(await status("POST", `/api/projects/${projectId}/pull`)).toBe(204);

  const plain = await api<{ id: string; is_git: boolean; path: string }>("POST", "/api/projects/add", { path: service.folder });
  await api("POST", `/api/projects/${plain.id}/trust`);
  const initialized = await api<{ id: string; is_git: boolean; path: string }>("POST", `/api/projects/${plain.id}/git/init`);
  expect(initialized.is_git).toBe(true);
  expect(existsSync(join(service.folder, ".git"))).toBe(true);

  const listed = await api<Worktrees>("GET", `/api/projects/${projectId}/worktrees`);
  expect(listed.git).toBe(true);
  expect(listed.worktrees.some((worktree) => worktree.is_main)).toBe(true);
  const worktreeThread = await createThread({ project_id: projectId, engine: engineId, worktree: "rpc-wt", title: "RPC worktree" });
  const grown = await api<Worktrees>("GET", `/api/projects/${projectId}/worktrees`);
  expect(grown.worktrees.length).toBe(listed.worktrees.length + 1);
  const worktree = grown.worktrees.find((entry) => !entry.is_main)!;
  expect(worktree.path).not.toBe(service.repo);

  // A change keeps the worktree when the thread is deleted, and the removal route refuses it.
  await api("PUT", `/api/threads/${worktreeThread.id}/files/content`, { path: "rpc-wt-file.txt", content: "kept\n", base_revision: null });
  expect(await status("DELETE", `/api/threads/${worktreeThread.id}`)).toBe(204);
  expect([400, 403]).toContain(await status("DELETE", `/api/projects/${projectId}/worktrees/${worktree.id}`));

  expect(await status("POST", `/api/projects/${projectId}/worktrees/${worktree.id}/file-actions`, { action: "delete", path: "rpc-wt-file.txt" })).toBe(204);
  expect(await status("DELETE", `/api/projects/${projectId}/worktrees/${worktree.id}`)).toBe(204);
  const cleaned = await api<Worktrees>("GET", `/api/projects/${projectId}/worktrees`);
  expect(cleaned.worktrees.length).toBe(listed.worktrees.length);
});
