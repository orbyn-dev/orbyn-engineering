import { api } from "../../../api/client";
import type { Thread } from "../../../api/types";
import { toFailure } from "../../../lib/useAsync";
import { DRAFT_KEY } from "../../../product/state/draftStore";
import { threadToken } from "../../../product/composer/skillMention";
import { useUi } from "../../../product/state/ui";
import { dropPatch, projectDrop, type DropSection, type DropTarget, type DropVerb, type SidebarSection, type ThreadSections } from "./groups";
import { reorderThreads, settleThreads, unpinThreads } from "./threadActions";

export interface DragState {
  id: string;
  from: SidebarSection;
  target: DropTarget | null;
}

export const sweepVerbs: Partial<Record<SidebarSection, DropVerb>> = {
  settled: "unsettle",
  snoozed: "wake",
};

/** The slot under the pointer, skipping the dragged row. Null means nowhere to drop. */
export function dropTargetAt(
  list: HTMLElement,
  y: number,
  dragged: string,
  sections: ThreadSections,
): DropTarget | null {
  let last: HTMLElement | null = null;
  for (const element of list.querySelectorAll<HTMLElement>("[data-drop-section], [data-thread-item]")) {
    if (element.dataset.threadItem === dragged) continue;
    if (element.getBoundingClientRect().top > y) break;
    last = element;
  }
  if (!last) return { section: "pinned", index: 0 };
  const marker = last.dataset.dropSection;
  if (marker) {
    return marker === "none" ? null : { section: marker as DropSection, index: 0 };
  }
  const section = last.dataset.section as SidebarSection;
  if (section === "working" || section === "snoozed") return null;
  const rows = sections[section].filter((thread) => thread.id !== dragged);
  const at = rows.findIndex((thread) => thread.id === last?.dataset.threadItem);
  const rect = last.getBoundingClientRect();
  return { section, index: at + (y > rect.top + rect.height / 2 ? 1 : 0) };
}

export const threadIdAt = (list: HTMLElement, y: number) => {
  let id: string | null = null;
  for (const row of list.querySelectorAll<HTMLElement>("[data-thread-item]")) {
    if (id !== null && row.getBoundingClientRect().top > y) break;
    id = row.dataset.threadItem ?? null;
  }
  return id;
};

export const sameTarget = (a: DropTarget | null, b: DropTarget | null) =>
  a?.section === b?.section && a?.index === b?.index;

/** What a finished drag needs from the sidebar: the latest sections and two store actions. */
export interface DropContext {
  sectionsRef: { current: ThreadSections };
  putThread(thread: Thread): void;
  pushToast(message: string): void;
}

export async function finishDrop(
  thread: Thread,
  from: SidebarSection,
  target: DropTarget,
  { sectionsRef, putThread, pushToast }: DropContext,
) {
  if (target.section === "settled") {
    if (from !== "settled") await settleThreads([thread]);
    return;
  }
  const ordered = projectDrop(sectionsRef.current, thread.id, target)[target.section];
  const patch = dropPatch(from, target.section);
  if (!patch) {
    const before = sectionsRef.current[target.section].map((row) => row.id).join();
    if (before !== ordered.map((row) => row.id).join()) await reorderThreads(ordered);
    return;
  }
  try {
    if (from === "pinned" && target.section === "active") await unpinThreads([thread]);
    else putThread(await api.patchThread(thread.id, patch));
    await reorderThreads(ordered);
  } catch (failure) {
    pushToast(`Couldn't move the thread. ${toFailure(failure).message}`);
  }
}

/** A thread dropped on a composer becomes a link the agent can read. */
export function mentionInComposer(rows: Thread[], selectedThread: string | null) {
  const key = selectedThread ?? DRAFT_KEY;
  const { drafts, setDraft } = useUi.getState();
  const links = rows.map((row) => threadToken(row.id, row.title)).join(" ");
  const before = drafts[key] ?? "";
  setDraft(key, before ? `${before.trimEnd()} ${links} ` : `${links} `);
}
