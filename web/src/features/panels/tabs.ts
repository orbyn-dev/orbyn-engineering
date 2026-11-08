import { Activity, FileDiff, Files, Globe, MessagesSquare, Plug } from "lucide-react";
import { CLOSED_AT_FIRST, RIGHT_TAB_IDS, type RightTab } from "../../state/ui";

/** The panel's six surfaces. `label` is the tab's name and what it reads. */
export const RIGHT_TABS: { id: RightTab; label: string; icon: typeof FileDiff }[] = [
  { id: "diff", label: "Diff", icon: FileDiff },
  { id: "files", label: "Files", icon: Files },
  { id: "browser", label: "Browser", icon: Globe },
  { id: "activity", label: "Activity", icon: Activity },
  { id: "side", label: "Side chat", icon: MessagesSquare },
  { id: "tools", label: "Tools", icon: Plug },
];

export const tabOf = (id: RightTab) => RIGHT_TABS.find((tab) => tab.id === id) ?? RIGHT_TABS[0]!;

/** True when the row is not the default: a tab was closed or opened against its default, or the order changed. */
export const tabsChanged = (order: readonly RightTab[], closed: readonly RightTab[]) =>
  closed.length !== CLOSED_AT_FIRST.length ||
  closed.some((id) => !CLOSED_AT_FIRST.includes(id)) ||
  order.some((id, at) => id !== RIGHT_TAB_IDS[at]);

/** One tab slot: 32 px wide, at every panel width, so an icon never moves when another tab opens. */
export const SLOT = 32;
/** The gap between slots in the strip. */
export const SLOT_GAP = 2;

/**
 * How many tab slots the strip has, from the room it leaves for slots and the "+" (and the "..." when
 * tabs fold). With room for every tab and the "+", all show; otherwise one slot goes to "...".
 */
export function tabSlots(room: number, count: number): number {
  const items = Math.max(0, Math.floor((room + SLOT_GAP) / (SLOT + SLOT_GAP)));
  const plain = items - 1;
  if (count <= plain) return count;
  return Math.max(1, plain - 1);
}

/**
 * The tabs shown in the strip, in order, and the rest for the "..." menu. The first `slots` tabs show.
 * A tab chosen from the menu (`promoted`) keeps the last slot, and the open tab always shows: when it
 * is not among the first `slots` it takes the last slot too. Nothing else ever changes a slot.
 */
export function visibleTabs(ids: readonly RightTab[], active: RightTab | null, promoted: RightTab | null, slots: number): { shown: RightTab[]; hidden: RightTab[] } {
  if (slots >= ids.length) return { shown: [...ids], hidden: [] };
  const base = ids.slice(0, Math.max(1, slots));
  const pick = active && ids.includes(active) && !base.includes(active) ? active : promoted && ids.includes(promoted) && !base.includes(promoted) ? promoted : null;
  const shown = pick ? [...base.slice(0, base.length - 1), pick] : base;
  return { shown, hidden: ids.filter((id) => !shown.includes(id)) };
}
