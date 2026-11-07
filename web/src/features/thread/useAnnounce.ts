import { useEffect, useRef, useState } from "react";
import type { ThreadItem } from "./projection";

const MAX_CHARS = 300;

/** What a screen reader should hear when an item arrives, or null for rows that stay quiet. */
export function announcementFor(item: ThreadItem): string | null {
  switch (item.kind) {
    case "message":
      return item.role !== "user" && item.done && item.text.trim() ? item.text.slice(0, MAX_CHARS) : null;
    case "notice":
      return item.tone === "error" ? `Error: ${item.text.slice(0, MAX_CHARS)}` : null;
    case "approval":
      return item.approval.state === "pending" ? `Needs your decision: ${item.approval.summary}` : null;
    default:
      return null;
  }
}

/**
 * Words for a polite live region, taken from items as they arrive in the log. The transcript's
 * own region is switched off because rows mount and unmount as the view moves, and that is not
 * news; a message that lands out of the window is still news.
 */
export function useAnnounce(items: readonly ThreadItem[]): string {
  const [spoken, setSpoken] = useState("");
  const heard = useRef<Set<string> | null>(null);
  useEffect(() => {
    if (heard.current === null) {
      // What was already there when the thread opened is not announced; a reply still streaming is.
      heard.current = new Set(items.filter((item) => announcementFor(item) !== null).map((item) => item.key));
      return;
    }
    const fresh: string[] = [];
    for (const item of items) {
      const said = announcementFor(item);
      if (said === null || heard.current.has(item.key)) continue;
      heard.current.add(item.key);
      fresh.push(said);
    }
    if (fresh.length > 0) setSpoken(fresh.slice(-3).join(". "));
  }, [items]);
  return spoken;
}
