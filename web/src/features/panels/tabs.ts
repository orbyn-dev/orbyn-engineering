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

/**
 * Which tabs stay in the row when the panel is narrow; the rest fold into More. The open tab always
 * stays. `room` is the row's width when no More button sits beside it, `moreWidth` what that button
 * takes. Tabs keep their order, and a tab with no measured width is kept.
 */
export function fitTabs(ids: readonly RightTab[], widths: ReadonlyMap<RightTab, number>, room: number, moreWidth: number, open: RightTab | null): RightTab[] {
  const width = (id: RightTab) => widths.get(id) ?? 0;
  if (ids.reduce((sum, id) => sum + width(id), 0) <= room) return [...ids];
  let left = room - moreWidth - (open && ids.includes(open) ? width(open) : 0);
  const kept = new Set<RightTab>(open && ids.includes(open) ? [open] : []);
  for (const id of ids) {
    if (kept.has(id)) continue;
    if (width(id) > left) break;
    left -= width(id);
    kept.add(id);
  }
  return ids.filter((id) => kept.has(id));
}
