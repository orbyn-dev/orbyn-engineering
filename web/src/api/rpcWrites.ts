// Typed callers for the guarded RPC routes of #822 (R1 file mutate, R5 VCS), wired by #819. `request` adds the person key on
// the two Person routes (`personKey.ts` lists them) and on no other. No screen calls these yet: see the R1 and R5 rows in
// docs/parity.md for why the interface keeps its older paths.
import { request } from "./client";
import type { CreateBranch } from "../generated/CreateBranch";
import type { ExplorerFileAction } from "../generated/ExplorerFileAction";
import type { SwitchBranch } from "../generated/SwitchBranch";
import type { VcsStatus } from "../generated/VcsStatus";

const q = encodeURIComponent;

export const rpcWritesApi = {
  /** Person. Creates a file or folder, moves or renames, or deletes a file or empty folder in the thread's own checkout. */
  mutateFile: (threadId: string, action: ExplorerFileAction) =>
    request<void>("POST", `/api/threads/${q(threadId)}/files/mutate`, action),
  /** Person. Removes a linked worktree Orbyn made, by the opaque id of a fresh listing. Refused while the worktree holds changed, untracked or ignored files, or a thread; commits and branches are never touched. */
  removeWorktree: (projectId: string, worktreeId: string) =>
    request<void>("DELETE", `/api/projects/${q(projectId)}/worktrees/${q(worktreeId)}`),
  /** Local. The checkout's branch, tracking ref and working-tree counts, read now. */
  refreshVcsStatus: (threadId: string) =>
    request<VcsStatus>("POST", `/api/threads/${q(threadId)}/vcs/refresh-status`),
  /** Local. Creates a branch at an optional start and leaves the checkout where it is. */
  createRef: (threadId: string, name: string, start?: string) =>
    request<void>("POST", `/api/threads/${q(threadId)}/vcs/refs`, { name, ...(start === undefined ? {} : { start }) } satisfies CreateBranch),
  /** Local. Moves the checkout to an existing ref. */
  switchRef: (threadId: string, name: string) =>
    request<void>("POST", `/api/threads/${q(threadId)}/vcs/refs/switch`, { name } satisfies SwitchBranch),
};
