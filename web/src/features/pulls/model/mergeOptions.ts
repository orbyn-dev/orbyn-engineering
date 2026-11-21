import { create } from "zustand";

const DELETE_KEY = (projectId: string) => `orbyn.pulls.deleteBranch.${projectId}`;
/** "Delete branch after merge" is off until chosen, and then remembered for the project. */
export function deleteBranchChoice(projectId: string): boolean {
  try {
    return localStorage.getItem(DELETE_KEY(projectId)) === "1";
  } catch {
    return false;
  }
}
export function rememberDeleteBranch(projectId: string, on: boolean) {
  try {
    localStorage.setItem(DELETE_KEY(projectId), on ? "1" : "0");
  } catch {
    /* A private window keeps nothing: the choice then lasts for this page. */
  }
}

interface Held {
  /** The pull request these choices belong to; another pull request starts afresh. */
  key: string | null;
  auto: boolean;
  deleteBranch: boolean;
}
const useHeld = create<Held>(() => ({ key: null, auto: false, deleteBranch: false }));

export function resetMergeOptions() {
  useHeld.setState({ key: null, auto: false, deleteBranch: false });
}

/**
 * Merge when checks pass and Delete the branch: the merge menu and the merge dialog offer the same two options, so they
 * read and write this one state, for the pull request being merged. It is plain page state: nothing is written anywhere
 * (the delete choice is remembered by whoever merges), and it resets when the pull request changes.
 */
export function useMergeOptions(projectId: string, number: number) {
  const key = `${projectId}#${number}`;
  const held = useHeld();
  const mine = held.key === key;
  const auto = mine ? held.auto : false;
  const deleteBranch = mine ? held.deleteBranch : deleteBranchChoice(projectId);
  const set = (change: Partial<Pick<Held, "auto" | "deleteBranch">>) =>
    useHeld.setState((now) => (now.key === key ? change : { key, auto: false, deleteBranch: deleteBranchChoice(projectId), ...change }));
  return {
    auto,
    deleteBranch,
    setAuto: (value: boolean) => set({ auto: value }),
    setDeleteBranch: (value: boolean) => set({ deleteBranch: value }),
  };
}
