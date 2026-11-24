import { ArrowUp, Check, ChevronDown, ChevronRight, MoreHorizontal, RotateCw, Copy, Search, SlidersHorizontal, Square, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type KeyboardEvent, type MouseEvent, type ReactNode, type RefObject } from "react";
import { useUi } from "../../../../product/state/ui";
import { Button, IconButton } from "../../../../ui/controls";
import { Menu, type MenuItem } from "../../../../ui/Menu";
import { Presence } from "../../../../ui/Presence";
import type { BlockTerminal, CommandBlock, CommandBlocksApi } from "./blocksContract";
import { cleanCommand, durationWords, filterBlockLines, findTextMatches, lastLines, rerunText, runningClock, type TextMatch } from "./blockText";
import { readBlockOutput, type BlockOutput } from "./readOutput";

/** Lines a block shows before "Show all". */
const SHOWN_LINES = 6;
const NO_COMMAND = "Command not reported";

type Reading = { status: "loading" } | { status: "error" } | { status: "ready"; output: BlockOutput };

export const isRunning = (block: CommandBlock) => block.ended_at === null;
export const isFailed = (block: CommandBlock) => block.ended_at !== null && block.exit_code !== null && block.exit_code !== 0;

function metaOf(block: CommandBlock, now: number): string {
  if (block.ended_at === null) return runningClock(block.started_at, now);
  const took = durationWords(block.started_at, block.ended_at);
  const code = block.exit_code === null ? "no exit code" : `exit ${block.exit_code}`;
  return took === "" ? code : `${code} · ${took}`;
}

export interface BlockCardProps {
  api: CommandBlocksApi;
  terminal: BlockTerminal;
  block: CommandBlock;
  /** A running block reads its output again each time this changes. */
  tick: number;
  now: number;
  selected: boolean;
  collapsed: boolean;
  showAll: boolean;
  onSelect(): void;
  onToggle(): void;
  onShowAll(): void;
  onCollapseAll(): void;
  /** Types text into the terminal's input; absent when the terminal cannot take a write. */
  write?: ((text: string) => void) | undefined;
  /** Sends Ctrl-C to the terminal. */
  interrupt?: (() => void) | undefined;
}

/** One command's block: header, output as plain text, and the person's actions on it. */
export function BlockCard({ api, terminal, block, tick, now, selected, collapsed, showAll, onSelect, onToggle, onShowAll, onCollapseAll, write, interrupt }: BlockCardProps) {
  const pushToast = useUi((state) => state.pushToast);
  const running = isRunning(block);
  const failed = isFailed(block);
  const command = cleanCommand(block.command);
  const rerun = rerunText(block.command);
  const [reading, setReading] = useState<Reading>({ status: "loading" });
  const [retry, setRetry] = useState(0);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  /** The filter bar's text; null while the bar is closed. Empty keeps every line. */
  const [filter, setFilter] = useState<string | null>(null);
  /** The find bar's text; null while the bar is closed. The field keeps its text on no match. */
  const [find, setFind] = useState<string | null>(null);
  const [findAt, setFindAt] = useState(0);
  const filterField = useRef<HTMLInputElement>(null);
  const findField = useRef<HTMLInputElement>(null);
  const filterControl = useRef<HTMLButtonElement>(null);
  const findControl = useRef<HTMLButtonElement>(null);
  const copying = useRef(false);
  const keeps = !block.fullscreen && !collapsed;
  const refresh = running ? tick : 0;
  const text = reading.status === "ready" ? reading.output.text : "";
  const filtered = useMemo(() => (filter === null ? null : filterBlockLines(text, filter)), [filter, text]);
  const matches = useMemo(() => (find === null ? [] : findTextMatches(text, find)), [find, text]);
  const current = matches.length === 0 ? -1 : Math.min(findAt, matches.length - 1);
  const filterOpen = filter !== null;
  const findOpen = find !== null;

  useEffect(() => {
    if (!keeps) return;
    const controller = new AbortController();
    readBlockOutput(api, terminal, block.id, controller.signal).then(
      (output) => {
        if (!controller.signal.aborted) setReading({ status: "ready", output });
      },
      () => {
        if (!controller.signal.aborted) setReading((now) => (now.status === "ready" ? now : { status: "error" }));
      },
    );
    return () => controller.abort();
    // A finished block is read once; a running one again at every tick; a change in its end or size reads it again too.
  }, [api, terminal, block.id, block.ended_at, block.output_bytes, keeps, refresh, retry]);

  // A collapsed block keeps no filter and no find bar, and shows their control unpressed again.
  useEffect(() => {
    if (!collapsed) return;
    setFilter(null);
    setFind(null);
  }, [collapsed]);
  // Opening a bar puts the keyboard in its field; expanding a collapsed block afterwards does the same when the bar first mounts.
  useEffect(() => {
    if (!filterOpen || collapsed) return;
    filterField.current?.focus();
    filterField.current?.select();
  }, [filterOpen, collapsed]);
  useEffect(() => {
    if (!findOpen || collapsed) return;
    findField.current?.focus();
    findField.current?.select();
  }, [findOpen, collapsed]);

  const say = (text: string) => pushToast(text);
  const clip = (text: string, done: string, failedText: string) => {
    const clipboard = navigator.clipboard;
    if (!clipboard) return say(failedText);
    clipboard.writeText(text).then(
      () => say(done),
      () => say(failedText),
    );
  };
  const readAll = async (): Promise<string | null> => {
    try {
      return (await readBlockOutput(api, terminal, block.id, new AbortController().signal)).text;
    } catch {
      return null;
    }
  };
  /** One copy at a time per block: a second press while the first reads does nothing. */
  const copyOutput = async (withCommand: boolean) => {
    if (copying.current) return;
    copying.current = true;
    try {
      const text = await readAll();
      if (text === null) return say("Couldn't read the output");
      clip(withCommand && command !== null ? `$ ${command}\n${text}` : text, withCommand ? "Command and output copied" : "Output copied", "Couldn't copy the output");
    } finally {
      copying.current = false;
    }
  };
  const copyCommand = () => (command === null ? undefined : clip(command, "Command copied", "Couldn't copy the command"));
  const runAgain = () => {
    if (rerun !== null) write?.(rerun);
  };

  /** The header controls: one bar at a time, and a collapsed block opens so the bar can show. */
  const openFilter = () => {
    if (collapsed) onToggle();
    setFind(null);
    setFilter("");
  };
  const openFind = () => {
    if (collapsed) onToggle();
    setFilter(null);
    setFind("");
    setFindAt(0);
  };
  const closeFilter = () => {
    setFilter(null);
    filterControl.current?.focus();
  };
  const closeFind = () => {
    setFind(null);
    findControl.current?.focus();
  };
  const toggleFilter = () => (filterOpen ? closeFilter() : openFilter());
  const toggleFind = () => (findOpen ? closeFind() : openFind());
  const stepFind = (by: 1 | -1) => {
    if (matches.length === 0) return;
    setFindAt((at) => (Math.min(at, matches.length - 1) + by + matches.length) % matches.length);
  };

  const filterCount = filtered === null
    ? ""
    : `${filtered.kept.toLocaleString("en-US")} of ${filtered.total.toLocaleString("en-US")} ${filtered.total === 1 ? "line" : "lines"}`;
  const findCount = find === null || find === "" ? "" : matches.length === 0 ? "No matches" : `${current + 1} of ${matches.length}`;

  const stop = (action: () => void) => (event: MouseEvent) => {
    event.stopPropagation();
    action();
  };
  const onKey = (event: KeyboardEvent) => {
    // Esc clears the open bar even when the keyboard is on the block or one of its header controls.
    if (event.key === "Escape" && (filterOpen || findOpen)) {
      event.preventDefault();
      event.stopPropagation();
      if (filterOpen) closeFilter();
      else closeFind();
      return;
    }
    if (event.target !== event.currentTarget) return;
    if (event.key === "Enter") {
      event.preventDefault();
      onToggle();
    } else if (event.key === " ") {
      event.preventDefault();
      onSelect();
    }
  };
  const canRerun = rerun !== null && write !== undefined && !running;
  const items: MenuItem[] = [
    { label: "Copy command", disabled: command === null, onSelect: copyCommand },
    { label: "Copy output", disabled: block.fullscreen, onSelect: () => void copyOutput(false) },
    { label: "Copy both", disabled: block.fullscreen || command === null, onSelect: () => void copyOutput(true) },
    { label: "Run again", disabled: !canRerun, separatorBefore: true, onSelect: runAgain },
    { label: collapsed ? "Expand" : "Collapse", separatorBefore: true, onSelect: onToggle },
    { label: "Collapse all", onSelect: onCollapseAll },
  ];

  return (
    <div
      className="cb-block"
      role="group"
      aria-label={command === null ? NO_COMMAND : `Command ${command}`}
      tabIndex={0}
      data-failed={failed || undefined}
      data-running={running || undefined}
      data-selected={selected || undefined}
      onClick={onSelect}
      onKeyDown={onKey}
    >
      <div className="cb-head">
        <span className="cb-mark" data-tone={failed ? "failed" : undefined}>
          {running ? <i className="spin" aria-label="Running" /> : failed ? <X size={12} aria-hidden /> : <Check size={12} aria-hidden />}
        </span>
        <span className="cb-cmd" title={command ?? NO_COMMAND} data-quiet={command === null || undefined}>
          {command ?? NO_COMMAND}
        </span>
        <span className="cb-meta" data-tone={failed ? "failed" : undefined}>
          {metaOf(block, now)}
        </span>
        <span className="cb-acts" role="toolbar" aria-label="Block actions">
          {block.fullscreen ? null : (
            <IconButton label="Copy output" onClick={stop(() => void copyOutput(false))}>
              <Copy size={14} aria-hidden />
            </IconButton>
          )}
          <IconButton label="Run again" disabled={!canRerun} onClick={stop(runAgain)}>
            <RotateCw size={14} aria-hidden />
          </IconButton>
          {block.fullscreen ? null : (
            <>
              <IconButton ref={filterControl} label="Filter output" pressed={filterOpen} onClick={stop(toggleFilter)}>
                <SlidersHorizontal size={14} aria-hidden />
              </IconButton>
              <IconButton ref={findControl} label="Find in output" pressed={findOpen} onClick={stop(toggleFind)}>
                <Search size={14} aria-hidden />
              </IconButton>
            </>
          )}
          <IconButton className="cb-fold" label={collapsed ? "Expand" : "Collapse"} onClick={stop(onToggle)}>
            {collapsed ? <ChevronRight size={14} aria-hidden /> : <ChevronDown size={14} aria-hidden />}
          </IconButton>
          <IconButton
            label="More"
            pressed={menu !== null}
            onClick={(event) => {
              event.stopPropagation();
              const rect = event.currentTarget.getBoundingClientRect();
              setMenu({ x: rect.left, y: rect.bottom + 4 });
            }}
          >
            <MoreHorizontal size={14} aria-hidden />
          </IconButton>
        </span>
      </div>
      {collapsed ? null : block.fullscreen ? (
        <div className="cb-note">Full-screen program, output not kept</div>
      ) : (
        <>
          {filterOpen ? (
            <FilterBar value={filter} count={filterCount} field={filterField} onChange={setFilter} onClose={closeFilter} />
          ) : findOpen ? (
            <FindBar value={find} count={findCount} field={findField} hasMatches={matches.length > 0} onChange={(value) => { setFind(value); setFindAt(0); }} onStep={stepFind} onClose={closeFind} />
          ) : null}
          <Output
            reading={reading}
            showAll={showAll}
            running={running}
            block={block}
            onShowAll={onShowAll}
            onRetry={() => setRetry(retry + 1)}
            filtered={filtered}
            findOpen={findOpen}
            matches={matches}
            current={current}
          />
        </>
      )}
      {!collapsed && running && interrupt ? (
        <div className="cb-foot">
          <Button small onClick={stop(interrupt)}>
            <Square size={12} aria-hidden />
            Stop
          </Button>
        </div>
      ) : null}
      <Presence>{menu ? <Menu x={menu.x} y={menu.y} label="Block menu" items={items} onClose={() => setMenu(null)} /> : null}</Presence>
    </div>
  );
}

/** The filter bar: plain text, the count of the block's lines that remain, and Show all to clear it. */
function FilterBar({ value, count, field, onChange, onClose }: {
  value: string;
  count: string;
  field: RefObject<HTMLInputElement | null>;
  onChange(value: string): void;
  onClose(): void;
}) {
  return (
    <div
      className="cb-bar"
      role="search"
      aria-label="Filter output"
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          onClose();
        }
      }}
    >
      <SlidersHorizontal size={14} aria-hidden />
      <label className="cb-in">
        <input ref={field} aria-label="Filter output" value={value} onChange={(event) => onChange(event.target.value)} />
      </label>
      <output className="cb-count" aria-live="polite">{count}</output>
      <Button small tone="quiet" onClick={(event) => { event.stopPropagation(); onClose(); }}>
        Show all
      </Button>
    </div>
  );
}

/** The find bar: its text, where the current match is, and Previous, Next and Close. */
function FindBar({ value, count, field, hasMatches, onChange, onStep, onClose }: {
  value: string;
  count: string;
  field: RefObject<HTMLInputElement | null>;
  hasMatches: boolean;
  onChange(value: string): void;
  onStep(step: 1 | -1): void;
  onClose(): void;
}) {
  return (
    <div
      className="cb-bar"
      role="search"
      aria-label="Find in output"
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          onClose();
        }
      }}
    >
      <Search size={14} aria-hidden />
      <label className="cb-in">
        <input
          ref={field}
          aria-label="Find in output"
          value={value}
          onChange={(event) => onChange(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              onStep(event.shiftKey ? -1 : 1);
            }
          }}
        />
      </label>
      <output className="cb-count" aria-live="polite">{count}</output>
      <IconButton label="Previous match" disabled={!hasMatches} onClick={(event) => { event.stopPropagation(); onStep(-1); }}>
        <ArrowUp size={14} aria-hidden />
      </IconButton>
      <IconButton label="Next match" disabled={!hasMatches} onClick={(event) => { event.stopPropagation(); onStep(1); }}>
        <ChevronDown size={14} aria-hidden />
      </IconButton>
      <IconButton label="Close find" onClick={(event) => { event.stopPropagation(); onClose(); }}>
        <X size={14} aria-hidden />
      </IconButton>
    </div>
  );
}

/** The block's text with every find match marked; the current one carries `data-current` and is filled by the stylesheet. */
function markedText(text: string, matches: readonly TextMatch[], current: number): ReactNode {
  if (matches.length === 0) return text;
  const parts: ReactNode[] = [];
  let at = 0;
  matches.forEach((match, index) => {
    if (match.start > at) parts.push(text.slice(at, match.start));
    parts.push(
      <mark key={`${match.start}:${match.end}`} className="cb-find" data-current={index === current || undefined}>
        {text.slice(match.start, match.end)}
      </mark>,
    );
    at = match.end;
  });
  if (at < text.length) parts.push(text.slice(at));
  return parts;
}

function Output({ reading, showAll, running, block, onShowAll, onRetry, filtered, findOpen, matches, current }: {
  reading: Reading;
  showAll: boolean;
  running: boolean;
  block: CommandBlock;
  onShowAll(): void;
  onRetry(): void;
  /** The lines the filter keeps, or null while its bar is closed. */
  filtered: ReturnType<typeof filterBlockLines> | null;
  findOpen: boolean;
  matches: TextMatch[];
  current: number;
}) {
  if (reading.status === "error") {
    return (
      <div className="cb-note">
        <span>Couldn't read the output.</span>
        <Button small tone="quiet" onClick={(event) => { event.stopPropagation(); onRetry(); }}>
          Try again
        </Button>
      </div>
    );
  }
  if (reading.status === "loading") return <div className="cb-note">Reading the output…</div>;
  const { output } = reading;
  if (output.text === "") {
    if (filtered !== null || findOpen) return null;
    return running ? null : <div className="cb-note">No output</div>;
  }
  // Filter and find read the whole text the block holds, so a match past the six-line tail is neither hidden nor uncounted.
  if (filtered !== null) return <pre className="cb-out">{filtered.text}</pre>;
  if (findOpen) return <pre className="cb-out">{markedText(output.text, matches, current)}</pre>;
  const tail = showAll ? null : lastLines(output.text, SHOWN_LINES);
  const cut = tail?.cut === true;
  const total = tail?.total ?? lastLines(output.text, Number.MAX_SAFE_INTEGER).total;
  const dropped = Math.max(output.dropped_lines, block.dropped_lines);
  const notes = [
    cut ? `Showing the last ${SHOWN_LINES} of ${total.toLocaleString("en-US")} lines` : null,
    dropped > 0 ? `${dropped.toLocaleString("en-US")} earlier ${dropped === 1 ? "line was" : "lines were"} dropped` : null,
    output.capped ? "Only the end is kept here" : null,
  ].filter((note): note is string => note !== null);
  return (
    <>
      <pre className="cb-out" data-faded={cut || undefined}>{tail ? tail.text : output.text}</pre>
      {notes.length > 0 || showAll ? (
        <div className="cb-more">
          {notes.length > 0 ? <span>{notes.join(" · ")}</span> : null}
          {cut ? (
            <Button small tone="quiet" onClick={(event) => { event.stopPropagation(); onShowAll(); }}>
              Show all
            </Button>
          ) : null}
        </div>
      ) : null}
    </>
  );
}
