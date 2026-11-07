import { ArrowDown } from "lucide-react";
import { Fragment, memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useThreadLogs, type LocalPrompt } from "./threadLogs";
import { useUi } from "../../state/ui";
import { Button } from "../../ui/controls";
import { Empty, ErrorState, Loading } from "../../ui/states";
import { TimelineRow, TimelineThread } from "./items";
import { Minimap, type MinimapItem } from "./Minimap";
import type { MessageItem, ThreadItem } from "./projection";
import { rowOfItems } from "./threadSearch";
import { arrangeTimeline, entryForMessage, entryIndexOf, firstUnread, runningTools } from "./timeline";
import { registerTranscript } from "./transcriptRegistry";
import { OpenStatePrefix, OpenStateScope } from "../../ui/openState";
import { useAnnounce } from "./useAnnounce";
import { useHeldRows } from "./useHeldRows";
import { useRowWindow, type Anchor } from "./useRowWindow";

const NEAR_BOTTOM = 80;
const NO_RUNS = {};
const REVEAL_MS = 2000;
/** Where each thread was left scrolled up, as a row and its distance from the top of the view. */
const scrollMemory = new Map<string, Anchor>();
const Row = memo(TimelineRow);

/** Shows accepted prompts the log hasn't echoed, at the start of their run. */
export function withLocalPrompts(
  items: readonly ThreadItem[],
  prompts: readonly LocalPrompt[],
): readonly ThreadItem[] {
  let merged = items;
  for (const prompt of prompts) {
    const echoed = merged.some(
      (item) => item.kind === "message" && item.role === "user" && item.runId === prompt.runId,
    );
    if (echoed) continue;
    const local: MessageItem = {
      kind: "message",
      key: `local:${prompt.runId}`,
      seq: 0,
      ts: "",
      runId: prompt.runId,
      role: "user",
      text: prompt.text,
      done: true,
    };
    const at = merged.findIndex((item) => item.runId === prompt.runId);
    merged =
      at === -1 ? [...merged, local] : [...merged.slice(0, at), local, ...merged.slice(at)];
  }
  return merged;
}

export function Transcript({
  threadId,
  active,
  activeRunId,
  header,
  footer,
  readUpTo,
}: {
  threadId: string;
  active: boolean;
  activeRunId?: string | null;
  /** Rows after this time are new to you, and a line marks where they start. */
  readUpTo?: string | null;
  /** The run status line, which sits at the end of the conversation. */
  /** Pinned at the top of the column while the transcript scrolls. */
  header?: ReactNode;
  footer?: ReactNode;
}) {
  const log = useThreadLogs((state) => state.logs[threadId]);
  const stream = useThreadLogs((state) => state.streams[threadId]);
  const prompts = useThreadLogs((state) => state.prompts[threadId]);
  const items = useMemo(
    () => withLocalPrompts(log?.items ?? [], prompts ?? []),
    [log, prompts],
  );

  const entries = useMemo(
    () => arrangeTimeline(items, log?.runs ?? NO_RUNS, activeRunId ?? null),
    [items, log?.runs, activeRunId],
  );
  const tools = useMemo(() => (active ? runningTools(items) : []), [items, active]);
  const turnPrompts = useMemo(
    () =>
      items.flatMap((item): MinimapItem[] => {
        if (item.kind === "message" && item.role === "user") return [{ key: item.key, text: item.text, kind: "you" }];
        if (item.kind === "notice" && item.tone === "error") return [{ key: item.key, text: item.text, kind: "failed" }];
        if (item.kind === "approval" && item.approval.state === "pending") {
          return [{ key: item.key, text: item.approval.summary, kind: "ask" }];
        }
        return [];
      }),
    [items],
  );

  const [arrivedAt] = useState(() => new Date().toISOString());
  const newKey = useMemo(() => firstUnread(items, readUpTo ?? null, arrivedAt), [items, readUpTo, arrivedAt]);
  const newAt = newKey ? entryIndexOf(entries, newKey) : -1;

  const scroller = useRef<HTMLDivElement>(null);
  const inner = useRef<HTMLDivElement>(null);
  const [restore] = useState(() => scrollMemory.get(threadId) ?? null);
  const pinned = useRef(restore === null);
  const [following, setFollowing] = useState(restore === null);
  const setPinned = useCallback((value: boolean) => {
    pinned.current = value;
    setFollowing(value);
  }, []);
  const unpin = useCallback(() => setPinned(false), [setPinned]);

  const indexByKey = useMemo(() => new Map(entries.map((entry, at) => [entry.key, at])), [entries]);
  const entryKeys = useMemo(() => entries.map((entry) => entry.key), [entries]);
  const rowOfItem = useMemo(() => rowOfItems(entries), [entries]);
  const held = useHeldRows(scroller, indexByKey, entryKeys);
  const win = useRowWindow({ threadId, entries, scroller, pinned, following, indexByKey, held, restore, onUnpin: unpin });
  const { scrollToIndex, offsetOfRow, onScroll: windowScrolled } = win;
  const spoken = useAnnounce(items);

  const { stickToEnd } = win;
  const entriesNow = useRef(entries);
  const anchorNow = useRef(win.anchor);
  useLayoutEffect(() => {
    entriesNow.current = entries;
    anchorNow.current = win.anchor;
  });
  useEffect(() => {
    scrollMemory.delete(threadId);
    return () => {
      const at = anchorNow.current();
      if (!pinned.current && at) scrollMemory.set(threadId, at);
    };
  }, [threadId]);
  useEffect(
    () => registerTranscript(threadId, { entries: () => entriesNow.current, root: () => scroller.current, reveal: scrollToIndex }),
    [threadId, scrollToIndex],
  );

  // Pinned means pinned: expanding content, late fonts and streaming all stay in view, and
  // so does the end when the view itself shrinks, as when the composer grows or a question docks.
  useEffect(() => {
    const element = scroller.current;
    const content = inner.current;
    if (!element || !content || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      if (pinned.current) stickToEnd();
    });
    observer.observe(content);
    observer.observe(element);
    return () => observer.disconnect();
  }, [stickToEnd]);

  useLayoutEffect(() => {
    if (pinned.current) stickToEnd();
  }, [items, stickToEnd]);

  const reveal = useUi((state) => (state.messageReveal?.threadId === threadId ? state.messageReveal : null));
  const clearReveal = useUi((state) => state.clearMessageReveal);
  const revealTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(revealTimer.current), []);
  useEffect(() => {
    if (!reveal || entries.length === 0) return;
    const key = entryForMessage(entries, reveal.messageId);
    const at = key ? indexByKey.get(key) : undefined;
    if (at === undefined) return;
    clearReveal();
    void scrollToIndex(at, "center").then((row) => {
      row?.setAttribute("data-revealed", "true");
      clearTimeout(revealTimer.current);
      revealTimer.current = setTimeout(() => row?.removeAttribute("data-revealed"), REVEAL_MS);
    });
  }, [reveal, entries, indexByKey, clearReveal, scrollToIndex]);

  // Only a move up lets go of the end. A scroll event can arrive after the view has shrunk,
  // which leaves the end farther away without anyone having scrolled from it.
  // Rows also change height as they are measured. When the content shrinks under a view that was
  // at the end, the browser clamps the view up by at most that much without anyone having scrolled.
  const lastTop = useRef(0);
  const lastHeight = useRef(0);
  const wasAtEnd = useRef(restore === null);
  const onScroll = () => {
    const element = scroller.current;
    if (!element) return;
    const atEnd = element.scrollHeight - element.scrollTop - element.clientHeight < NEAR_BOTTOM;
    const shrunk = lastHeight.current - element.scrollHeight;
    const clamped = wasAtEnd.current && shrunk > 1 && lastTop.current - element.scrollTop <= shrunk + 1;
    const movedUp = element.scrollTop < lastTop.current - 1 && !clamped;
    lastTop.current = element.scrollTop;
    lastHeight.current = element.scrollHeight;
    wasAtEnd.current = atEnd;
    if (atEnd) setPinned(true);
    else if (movedUp) setPinned(false);
    windowScrolled();
  };

  const jump = () => {
    const element = scroller.current;
    if (!element) return;
    setPinned(true);
    element.scrollTop = element.scrollHeight;
    stickToEnd();
  };

  const locate = useCallback(
    (key: string) => {
      const at = rowOfItem.get(key);
      return at === undefined ? null : offsetOfRow(at);
    },
    [rowOfItem, offsetOfRow],
  );
  const goTo = useCallback(
    (key: string) => {
      const at = rowOfItem.get(key);
      if (at !== undefined) void scrollToIndex(at, "start");
    },
    [rowOfItem, scrollToIndex],
  );

  const status = stream?.status ?? "connecting";
  const empty = items.length === 0;

  return (
    <div className="transcript-wrap">
      <div
        ref={scroller}
        className="transcript"
        onScroll={onScroll}
        tabIndex={0}
        role="log"
        aria-label="Thread messages"
        aria-live="off"
        aria-busy={active || undefined}
      >
        <div ref={inner} className="transcript-inner">
          {header}
          {empty && status === "failed" && stream?.failure ? (
            <ErrorState title="Couldn't load this thread" failure={stream.failure} />
          ) : empty && (status === "connecting" || status === "reconnecting") ? (
            <Loading label="Loading messages" rows={3} />
          ) : empty ? (
            <Empty title="No messages yet">
              <p>Describe the task below to start a run.</p>
            </Empty>
          ) : (
            <TimelineThread value={threadId}>
              <OpenStateScope value={threadId}>
              {win.segments.map((segment) => {
                if (segment.kind === "gap") {
                  // Rows out of view: one spacer with the height they would fill.
                  return <div key={`gap:${entries[segment.from]!.key}`} aria-hidden style={{ height: segment.height, flex: "none" }} />;
                }
                const entry = entries[segment.index]!;
                return (
                  <Fragment key={entry.key}>
                    {segment.index === newAt ? (
                      <p className="visit-mark" role="separator">
                        New since you were here
                      </p>
                    ) : null}
                    <div ref={win.rowRef} className={`item item-${entry.kind}`} data-entry={entry.key}>
                      <OpenStatePrefix value={entry.key}>
                        <Row entry={entry} live={active} tools={tools} />
                      </OpenStatePrefix>
                    </div>
                  </Fragment>
                );
              })}
              </OpenStateScope>
            </TimelineThread>
          )}
          {footer}
        </div>
      </div>
      {/* The log above is not live (its rows mount as you scroll); this announces what arrives. */}
      <div className="sr-only" role="status" aria-live="polite" aria-atomic="true">
        {spoken}
      </div>
      <Minimap items={turnPrompts} scroller={scroller} locate={locate} goTo={goTo} revision={win.rev} />
      {!following ? (
        <Button small className="jump" icon={<ArrowDown size={16} aria-hidden />} onClick={jump}>
          Jump to latest
        </Button>
      ) : null}
    </div>
  );
}
