import type { ThreadItem } from "./projection";
import type { TimelineEntry } from "./timeline";

/** Text a person can find in one item, taken from the log rather than from what is mounted. */
function itemText(item: ThreadItem): string {
  switch (item.kind) {
    case "message":
    case "reasoning":
    case "notice":
      return item.text;
    case "tool":
      return `${item.name} ${item.args}`;
    case "approval":
      return item.approval.summary;
    case "plan":
      return item.steps.map((step) => step.text).join("\n");
    case "diff":
      return item.files.map((file) => file.path).join("\n");
    default:
      return "";
  }
}

const cache = new WeakMap<TimelineEntry, string>();

/** Everything an entry shows or holds behind a fold, lower-cased for matching. */
export function entryText(entry: TimelineEntry): string {
  const known = cache.get(entry);
  if (known !== undefined) return known;
  let text: string;
  switch (entry.kind) {
    case "work":
      text = entry.items.map(itemText).join("\n");
      break;
    case "fold":
      text = `${entry.label}\n${entry.entries.map(entryText).join("\n")}`;
      break;
    case "working":
      text = "";
      break;
    default:
      text = itemText(entry);
  }
  const lower = text.toLowerCase();
  cache.set(entry, lower);
  return lower;
}

export interface FindMatch {
  /** Row that holds the match. */
  index: number;
  /** Which occurrence inside that row, counted from zero. */
  nth: number;
}

/** Every occurrence of `needle` in order, over all rows, mounted or not. */
export function findMatches(entries: readonly TimelineEntry[], needle: string): FindMatch[] {
  if (!needle) return [];
  const lower = needle.toLowerCase();
  const found: FindMatch[] = [];
  entries.forEach((entry, index) => {
    const text = entryText(entry);
    let nth = 0;
    for (let at = text.indexOf(lower); at >= 0; at = text.indexOf(lower, at + lower.length)) {
      found.push({ index, nth });
      nth += 1;
    }
  });
  return found;
}

/** Which row holds each item, including items inside a fold. */
export function rowOfItems(entries: readonly TimelineEntry[]): Map<string, number> {
  const rows = new Map<string, number>();
  const visit = (entry: TimelineEntry, index: number) => {
    rows.set(entry.key, index);
    if (entry.kind === "fold") for (const inner of entry.entries) visit(inner, index);
    else if (entry.kind === "work") for (const item of entry.items) rows.set(item.key, index);
  };
  entries.forEach(visit);
  return rows;
}

/** The first row that holds an item the test accepts, or -1. */
export function rowWhere(entries: readonly TimelineEntry[], accept: (item: ThreadItem) => boolean): number {
  const holds = (entry: TimelineEntry): boolean => {
    if (entry.kind === "fold") return entry.entries.some(holds);
    if (entry.kind === "work") return entry.items.some(accept);
    if (entry.kind === "working") return false;
    return accept(entry);
  };
  return entries.findIndex(holds);
}
