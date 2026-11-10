import { useEffect, useRef, useState } from "react";
import type { ThreadItem } from "../../product/timeline/types";

/** How long the words stay in the region before it empties, so it is not found stale later. */
const CLEAR_MS = 5000;

/**
 * What a screen reader should hear when an item arrives, or null for rows that stay quiet. It says
 * that something arrived, not what: the text is in the log for reading, and repeating it here
 * would read it twice and duplicate it on the page.
 */
export function announcementFor(item: ThreadItem): string | null {
  switch (item.kind) {
    case "message":
      return item.role !== "user" && item.done && item.text.trim() ? "New reply from the agent" : null;
    case "notice":
      return item.tone === "error" ? "Something went wrong in this thread" : null;
    case "approval":
      return item.approval.state === "pending" ? "The agent needs your decision" : null;
    default:
      return null;
  }
}

export interface Spoken {
  text: string;
  /** Changes with every announcement, so the same words are spoken again. */
  n: number;
}

/**
 * Words for a polite live region, taken from items as they arrive in the log. The transcript's
 * own region is switched off because rows mount and unmount as the view moves, and that is not
 * news; a message that lands out of the window is still news. An item counts when it began after
 * the transcript opened, or when it was seen still streaming and has now finished (a reply you
 * switched into mid-way). History replays already finished, so it is never announced as new.
 */
export function useAnnounce(items: readonly ThreadItem[], arrivedAt: string): Spoken {
  const [spoken, setSpoken] = useState<Spoken>({ text: "", n: 0 });
  const heard = useRef(new Set<string>());
  /** Replies that were still streaming the last time they were looked at. */
  const streaming = useRef(new Set<string>());
  useEffect(() => {
    const fresh: string[] = [];
    for (const item of items) {
      if (item.kind === "message" && item.role !== "user" && !item.done) streaming.current.add(item.key);
      if (heard.current.has(item.key)) continue;
      const said = announcementFor(item);
      if (said === null || (item.ts <= arrivedAt && !streaming.current.has(item.key))) continue;
      heard.current.add(item.key);
      streaming.current.delete(item.key);
      fresh.push(said);
    }
    if (fresh.length > 0) setSpoken((prev) => ({ text: [...new Set(fresh)].join(". "), n: prev.n + 1 }));
  }, [items, arrivedAt]);
  useEffect(() => {
    if (!spoken.text) return;
    const timer = setTimeout(() => setSpoken((prev) => (prev.n === spoken.n ? { text: "", n: prev.n } : prev)), CLEAR_MS);
    return () => clearTimeout(timer);
  }, [spoken]);
  return spoken;
}
