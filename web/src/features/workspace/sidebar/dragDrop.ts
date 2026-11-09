import type { DropSection, DropTarget, DropVerb, SidebarSection, ThreadSections } from "./groups";

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
