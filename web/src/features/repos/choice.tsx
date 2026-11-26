import { ChevronDown, FolderGit2 } from "lucide-react";
import { useState } from "react";
import { create } from "zustand";
import { useThreadLogs } from "../../product/state/threadLogs";
import { useWorkspace } from "../../product/state/workspace";
import { Menu } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";
import { useThreadRepos } from "./api";
import type { ThreadRepo } from "./contract";
import { plain } from "./model";

/** Which repository each multi-repository thread's Git menu and pickers act on. Kept in memory for the session only. */
const useChosen = create<{ picked: Record<string, string>; pick(threadId: string, repoId: string | null): void }>((set) => ({
  picked: {},
  pick: (threadId, repoId) => set((state) => {
    const next = { ...state.picked };
    if (repoId === null) delete next[threadId]; else next[threadId] = repoId;
    return { picked: next };
  }),
}));

/** Puts the choice back to the primary (or another repository) from outside a component's render. */
export const chooseRepo = (threadId: string, repoId: string | null) => useChosen.getState().pick(threadId, repoId);

/** A name from the service, one line of plain text and at most this long. */
const NAME_CAP = 48;
export const repoLabel = (name: string) => { const text = plain(name); return text.length > NAME_CAP ? `${text.slice(0, NAME_CAP - 1)}…` : text; };

/**
 * The repository the Git menu and the branch and commit pickers act on. For an ordinary thread (or a thread in one repository) there
 * is nothing to choose: `multi` is false, `selected` is `null` and `repoId` is `undefined`, so no call carries a `repo_id`.
 * For a thread in several repositories `selected` is always one of them, the primary until the person picks another, and `repoId`
 * is the `repo_id` every read and write sends: `undefined` for the primary repository (the project's own, whose id is the
 * project's), the repository's id for any other.
 */
export interface RepoChoice {
  multi: boolean;
  repos: ThreadRepo[];
  selected: ThreadRepo | null;
  /** The thread's primary repository: the project's own (its id is the project's), else the service's first. Null without a choice. */
  primary: ThreadRepo | null;
  /** The `repo_id` every call for the selected repository sends: undefined for the primary repository, whose calls leave it out. */
  repoId: string | undefined;
  threadId: string;
  choose(repoId: string | null): void;
  reload(): void;
}

/** The choice over a repository list the caller already holds (the Changes panel reads it for its groups). */
export function usePickedRepo(threadId: string, repos: ThreadRepo[] | null, reload: () => void = () => {}): RepoChoice {
  const projectId = useWorkspace((state) => state.threads[threadId]?.project_id);
  const pickedId = useChosen((state) => state.picked[threadId]);
  const pick = useChosen((state) => state.pick);
  const list = repos ?? [];
  const multi = list.length >= 2;
  // The primary is the project's own repository; when the list does not hold it, the service's first (position 0) is the primary.
  const primary = multi ? list.find((repo) => repo.repo_id === projectId) ?? list[0]! : null;
  const selected = multi ? list.find((repo) => repo.repo_id === pickedId) ?? primary : null;
  const repoId = selected && primary && selected.repo_id !== primary.repo_id ? selected.repo_id : undefined;
  return { multi, repos: list, selected, primary, repoId, threadId, choose: (id) => pick(threadId, id), reload };
}

/** Reads the thread's repositories and the choice over them; an ordinary thread reads a `404` and gets no chooser and no `repo_id`. */
export function useRepoChoice(threadId: string): RepoChoice {
  const version = useThreadLogs((state) => state.diffVersion[threadId] ?? 0);
  const { repos, reload } = useThreadRepos(threadId, version);
  return usePickedRepo(threadId, repos, reload);
}

/**
 * The visible repository choice: one chip naming the selected repository, a menu of the thread's repositories under it. Draws nothing
 * for a thread that works in one repository or none. A repository that is not trusted can be chosen; its actions are then refused.
 */
export function RepoChooser({ choice }: { choice: RepoChoice }) {
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  if (!choice.multi || !choice.selected) return null;
  const name = repoLabel(choice.selected.name);
  return (
    <>
      <button type="button" className="repos-chip" aria-label={`Repository: ${name}`} aria-haspopup="menu" aria-expanded={menu !== null}
        onClick={(event) => { const box = event.currentTarget.getBoundingClientRect(); setMenu({ x: box.left, y: box.bottom + 4 }); }}>
        <FolderGit2 size={14} aria-hidden />{name}<ChevronDown size={12} aria-hidden />
      </button>
      <Presence>
        {menu ? (
          <Menu x={menu.x} y={menu.y} label="Repository" heading="Act on repository" onClose={() => setMenu(null)}
            items={choice.repos.map((repo) => ({
              label: repoLabel(repo.name),
              selected: repo.repo_id === choice.selected?.repo_id,
              leadCheck: true,
              ...(repo.trusted ? {} : { shortcut: "Not trusted" }),
              onSelect: () => choice.choose(repo.repo_id),
            }))} />
        ) : null}
      </Presence>
    </>
  );
}
