import { ChevronDown, ChevronUp, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { IconButton } from "../../ui/controls";
import { findMatches, type FindMatch } from "./threadSearch";
import { transcriptFor } from "./transcriptRegistry";

const HIGHLIGHT = "thread-find";
const CURRENT = "thread-find-current";

interface Highlights {
  set(name: string, value: unknown): void;
  delete(name: string): void;
}

const registry = () => (globalThis.CSS as unknown as { highlights?: Highlights } | undefined)?.highlights;
const makeHighlight = (ranges: Range[]) => new (globalThis as unknown as { Highlight: new (...r: Range[]) => unknown }).Highlight(...ranges);

export function rangesIn(root: Node, needle: string): Range[] {
  if (!needle) return [];
  const lower = needle.toLowerCase();
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const ranges: Range[] = [];
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const text = (node.textContent ?? "").toLowerCase();
    for (let at = text.indexOf(lower); at >= 0; at = text.indexOf(lower, at + lower.length)) {
      const range = document.createRange();
      range.setStart(node, at);
      range.setEnd(node, at + lower.length);
      ranges.push(range);
    }
  }
  return ranges;
}

const frame = () => new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

/** Opens the fold or work group a match sits behind, so its text is on screen. */
async function openAround(row: HTMLElement, needle: string, nth: number): Promise<Range | undefined> {
  const pick = () => {
    const ranges = rangesIn(row, needle);
    return ranges[Math.min(nth, ranges.length - 1)];
  };
  const shown = (range: Range | undefined) => (range?.startContainer.parentElement?.getClientRects().length ?? 0) > 0;
  for (const toggle of [".work-summary", ".trace-summary"]) {
    if (shown(pick())) break;
    const closed = row.querySelectorAll<HTMLElement>(`${toggle}[aria-expanded="false"]`);
    if (closed.length === 0) continue;
    for (const button of closed) button.parentElement?.dispatchEvent(new Event("reveal-setting"));
    await frame();
    await frame();
  }
  return pick();
}

/**
 * Find in thread: searches every row of the log, mounted or not, highlights the matches that
 * are on screen and steps through all of them, bringing far-off rows in as it goes.
 */
export function ThreadFind({ threadId, onClose }: { threadId: string; onClose(): void }) {
  const [text, setText] = useState("");
  const [at, setAt] = useState(0);
  const matches = useRef<FindMatch[]>([]);
  const [count, setCount] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const ticket = useRef(0);

  const paint = useCallback((needle: string) => {
    const css = registry();
    const root = transcriptFor(threadId)?.root();
    if (css && root) css.set(HIGHLIGHT, makeHighlight(rangesIn(root, needle)));
  }, [threadId]);

  const show = useCallback(
    async (index: number, needle: string) => {
      const handle = transcriptFor(threadId);
      const match = matches.current[index];
      const css = registry();
      if (!handle || !match) return;
      const turn = ++ticket.current;
      const row = await handle.reveal(match.index, "center");
      if (!row || turn !== ticket.current) return;
      const range = await openAround(row, needle, match.nth);
      if (turn !== ticket.current) return;
      paint(needle);
      if (!range) return;
      css?.set(CURRENT, makeHighlight([range]));
      range.startContainer.parentElement?.scrollIntoView({ block: "center" });
    },
    [threadId, paint],
  );

  useEffect(() => {
    input.current?.focus();
    return () => {
      registry()?.delete(HIGHLIGHT);
      registry()?.delete(CURRENT);
    };
  }, []);

  useEffect(() => {
    const handle = transcriptFor(threadId);
    matches.current = handle ? findMatches(handle.entries(), text) : [];
    setCount(matches.current.length);
    setAt(0);
    registry()?.delete(CURRENT);
    paint(text);
    if (matches.current.length > 0) void show(0, text);
  }, [text, threadId, show, paint]);

  // Rows come and go as the view moves, so the highlights follow what is mounted.
  useEffect(() => {
    const root = transcriptFor(threadId)?.root();
    if (!root || !text) return;
    let queued = 0;
    const repaint = () => {
      cancelAnimationFrame(queued);
      queued = requestAnimationFrame(() => paint(text));
    };
    root.addEventListener("scroll", repaint, { passive: true });
    return () => {
      cancelAnimationFrame(queued);
      root.removeEventListener("scroll", repaint);
    };
  }, [threadId, text, paint]);

  const step = (by: number) => {
    // The log may have grown since the last step, so count again.
    const handle = transcriptFor(threadId);
    if (handle) {
      matches.current = findMatches(handle.entries(), text);
      setCount(matches.current.length);
    }
    const total = matches.current.length;
    if (total === 0) return;
    const next = (at + by + total) % total;
    setAt(next);
    void show(next, text);
  };

  return (
    <div className="thread-find" role="search">
      <input
        ref={input}
        aria-label="Find in thread"
        placeholder="Find in thread"
        value={text}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            step(event.shiftKey ? -1 : 1);
          } else if (event.key === "Escape") {
            event.stopPropagation();
            onClose();
          }
        }}
      />
      <span className="thread-find-count" aria-live="polite">
        {text ? (count === 0 ? "0/0" : `${at + 1}/${count}`) : ""}
      </span>
      <IconButton label="Previous match" disabled={count === 0} onClick={() => step(-1)}>
        <ChevronUp size={14} aria-hidden />
      </IconButton>
      <IconButton label="Next match" disabled={count === 0} onClick={() => step(1)}>
        <ChevronDown size={14} aria-hidden />
      </IconButton>
      <IconButton label="Close find" onClick={onClose}>
        <X size={14} aria-hidden />
      </IconButton>
    </div>
  );
}
