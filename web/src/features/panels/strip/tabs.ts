import { Activity, FileDiff, Files, Globe, MessagesSquare, Plug } from "lucide-react";
import { CLOSED_AT_FIRST, RIGHT_TAB_IDS, type RightTab } from "../../../product/state/ui";

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

/** A slot and the gap after it. */
export const SLOT_PITCH = 34;

/**
 * How many tab slots the strip has. `room` is the strip's width less its padding and the end buttons;
 * every slot, the "+" and the "..." takes 34 px (32 and a 2 px gap). With room for every tab and the
 * "+", all show; otherwise one slot goes to "...".
 */
export function tabSlots(room: number, count: number): number {
  const items = Math.max(0, Math.floor(room / SLOT_PITCH));
  const plain = items - 1;
  if (count <= plain) return count;
  return Math.max(1, plain - 1);
}

/**
 * The tabs shown in the strip, in order, and the rest for the "..." menu. The first `slots` tabs show.
 * A tab chosen from the menu (`promoted`) keeps the last slot, and the open tab always shows: when it
 * is not among the first `slots` it takes the last slot too, and when it is the last of them nothing
 * takes the slot from it. `holder` is the tab now in the last slot against the order, or null.
 */
export function visibleTabs(ids: readonly RightTab[], active: RightTab | null, promoted: RightTab | null, slots: number): { shown: RightTab[]; hidden: RightTab[]; holder: RightTab | null } {
  if (slots >= ids.length) return { shown: [...ids], hidden: [], holder: null };
  const base = ids.slice(0, Math.max(1, slots));
  const last = base[base.length - 1];
  const hiddenActive = active && ids.includes(active) && !base.includes(active) ? active : null;
  const kept = promoted && ids.includes(promoted) && !base.includes(promoted) && active !== last ? promoted : null;
  const holder = hiddenActive ?? kept;
  const shown = holder ? [...base.slice(0, base.length - 1), holder] : base;
  return { shown, hidden: ids.filter((id) => !shown.includes(id)), holder };
}
