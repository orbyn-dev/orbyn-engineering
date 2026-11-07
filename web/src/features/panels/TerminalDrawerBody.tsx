import { useCommand } from "../../app/shortcuts";
import { Check, ChevronDown, ChevronUp, Eye, Maximize2, Minimize2, MessageSquare, Play, Plus, RotateCw, Search, Square, SquareTerminal, X } from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import { request } from "../../api/client";
import type { ProjectFileView, TerminalInfo } from "../../api/types";
import { elapsed } from "../../lib/format";
import { toFailure, useAsync } from "../../lib/useAsync";
import { useNow } from "../../lib/useNow";
import { useConnectionKind } from "../../state/connection";
import { useUi } from "../../state/ui";
import { Button, IconButton } from "../../ui/controls";
import { ConfirmDialog } from "../../ui/Dialog";
import { Menu, type MenuItem } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";
import { TerminalPanel } from "./TerminalPanel";
import { controlsOf, terminalKey, watchTerminal } from "./terminalControls";
import { captureFocusReturn } from "../../ui/focusReturn";
import { addTerminalChip } from "./terminalChip";
import type { ProjectScript } from "../../generated/ProjectScript";
import { runProjectAction } from "../thread/runProjectAction";
import { ProjectActionIcon } from "../../ui/ProjectActionIcon";

const MAIN = "main";
const q = encodeURIComponent;

export function groupTerminals(terminals: readonly TerminalInfo[]): TerminalInfo[][] {
  const groups = new Map<string, TerminalInfo[]>();
  for (const terminal of terminals) groups.set(terminal.group, [...(groups.get(terminal.group) ?? []), terminal]);
  return [...groups.values()];
}


/** What a terminal's tab marks: a spinner while a script runs, a check or a red cross once it has ended. */
function Marker({ terminal }: { terminal: TerminalInfo }) {
  if (terminal.id === MAIN) return null;
  if (terminal.alive) return terminal.script ? <i className="spin" aria-label="Working" /> : null;
  if (terminal.exit_code === 0) return <Check size={12} aria-label="Finished" />;
  return <X size={12} className="term-failed" aria-label="Failed" />;
}

/** Terminals still going: the shells aren't counted, only what a script or the agent started. */
export const runningCount = (terminals: readonly TerminalInfo[] | null | undefined) =>
  (terminals ?? []).filter((terminal) => terminal.id !== MAIN && terminal.alive && terminal.script !== null).length;

/** How often the closed drawer asks what still runs: 10 s, doubling to a minute after empty answers, and then a slow minute that never stops while it is mounted. */
const BAR_POLL_MS = 10_000;
const BAR_POLL_MAX_MS = 60_000;

/** The slim bar left under the thread when the drawer is closed and something is still running. */
export function TerminalBar({ threadId, onOpen }: { threadId: string; onOpen(): void }) {
  const [count, setCount] = useState(0);
  // Offline or with the service away there is nothing to ask.
  const away = useConnectionKind();
  // A terminal revealed for this thread (a script started, the agent ran something) starts the quick asking again.
  const revealed = useUi((state) => (state.terminalReveal?.threadId === threadId ? state.terminalReveal.nonce : 0));
  useEffect(() => {
    if (away) return;
    let live = true;
    let empty = 0;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const read = async () => {
      try {
        const terminals = await request<TerminalInfo[]>("GET", `/api/threads/${q(threadId)}/terminals`);
        if (!live) return;
        const running = runningCount(terminals);
        setCount(running);
        empty = running === 0 ? empty + 1 : 0;
      } catch {
        empty += 1;
      }
    };
    const next = () => {
      if (!live) return;
      timer = setTimeout(async () => {
        if (!document.hidden) await read();
        next();
      }, Math.min(BAR_POLL_MS * 2 ** Math.min(empty, 3), BAR_POLL_MAX_MS));
    };
    void read().then(next);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [threadId, away, revealed]);
  if (count === 0) return null;
  return (
    <div className="term-bar" role="group" aria-label="Terminal">
      <SquareTerminal size={14} aria-hidden />
      Terminal
      <span className="term-bar-dot" aria-hidden>
        ·
      </span>
      <i className="spin" aria-hidden />
      {count === 1 ? "1 process running" : `${count} processes running`}
      <IconButton className="term-bar-open" label="Open terminal" onClick={onOpen}>
        <ChevronUp size={14} aria-hidden />
      </IconButton>
    </div>
  );
}

/** Search in the scrollback of the terminal being shown: the matches are marked and the count says where you are. */
function TerminalSearch({ keyOf, focusSignal, onClose }: { keyOf: string; focusSignal: number; onClose(): void }) {
  const [text, setText] = useState("");
  const [results, setResults] = useState<{ index: number; count: number }>({ index: -1, count: 0 });
  const field = useRef<HTMLInputElement>(null);
  // Pressing the search key again while it is open comes back to the field.
  useEffect(() => {
    field.current?.focus();
    field.current?.select();
  }, [focusSignal]);
  // Hears the results when xterm registers, which may be after the field opens.
  useEffect(() => watchTerminal(keyOf, (controls) => controls.onResults(setResults)), [keyOf]);
  useEffect(() => () => controlsOf(keyOf)?.find("", "next"), [keyOf]);
  const find = (value: string, direction: "next" | "previous") => controlsOf(keyOf)?.find(value, direction);
  const count = text === "" ? "" : results.count === 0 ? "No matches" : `${results.index + 1} of ${results.count}`;
  return (
    <div className="term-search" role="search" aria-label="Search the terminal">
      <label className="term-search-field">
        <Search size={14} aria-hidden />
        <input
          ref={field}
          aria-label="Search the terminal"
          value={text}
          autoFocus
          onChange={(event) => {
            setText(event.target.value);
            find(event.target.value, "next");
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              find(text, event.shiftKey ? "previous" : "next");
            } else if (event.key === "Escape") {
              event.preventDefault();
              event.stopPropagation();
              onClose();
            }
          }}
        />
        <span className="term-search-count" aria-live="polite">
          {count}
        </span>
      </label>
      <IconButton label="Previous match" onClick={() => find(text, "previous")}>
        <ChevronUp size={14} aria-hidden />
      </IconButton>
      <IconButton label="Next match" onClick={() => find(text, "next")}>
        <ChevronDown size={14} aria-hidden />
      </IconButton>
      <IconButton label="Close search" onClick={onClose}>
        <X size={14} aria-hidden />
      </IconButton>
    </div>
  );
}

/** Terminal tabs, each a group of splits, and the project's scripts. */
export function TerminalDrawerBody({
  threadId,
  maximized = false,
  onToggleMaximize = () => undefined,
  onClose,
}: {
  threadId: string;
  maximized?: boolean;
  onToggleMaximize?(): void;
  /** Hides the drawer; what runs keeps running. */
  onClose(): void;
}) {
  const base = `/api/threads/${q(threadId)}`;
  const terminals = useAsync(`terminals:${threadId}`, (signal) =>
    request<TerminalInfo[]>("GET", `${base}/terminals`, undefined, signal),
  );
  const scripts = useAsync(`scripts:${threadId}`, (signal) =>
    request<ProjectFileView>("GET", `${base}/scripts`, undefined, signal),
  );
  const [active, setActive] = useState(MAIN);
  // A terminal opened here takes focus when it loads; one that was already there does not.
  const [focusGroup, setFocusGroup] = useState<string | null>(null);
  const [mainHidden, setMainHidden] = useState(false);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [watching, setWatching] = useState(false);
  const [searching, setSearching] = useState(false);
  const [searchFocus, setSearchFocus] = useState(0);
  const [ending, setEnding] = useState<TerminalInfo | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const pushToast = useUi((state) => state.pushToast);
  const { reload } = terminals;
  const reveal = useUi((state) => state.terminalReveal);
  useEffect(() => {
    if (reveal?.threadId !== threadId) return;
    setActive(reveal.group); reload();
  }, [reveal, threadId, reload]);
  const groups = useMemo(() => groupTerminals(terminals.data ?? []), [terminals.data]);
  const tabs = groups.filter((group) => group[0]?.group !== MAIN);
  // With the main shell's tab closed, the first other tab is the one showing.
  const current = mainHidden && active === MAIN ? (tabs[0]?.[0]?.group ?? null) : active;
  const empty = current === null;
  const shown = groups.find((group) => group[0]?.group === current);
  const fail = useCallback(
    (failure: unknown) => pushToast(`Couldn't do that. ${toFailure(failure).message}`),
    [pushToast],
  );

  useEffect(() => {
    if (active !== MAIN && terminals.status === "ready" && !terminals.refreshing && !shown) setActive(MAIN);
  }, [active, shown, terminals.status, terminals.refreshing]);

  const open = async (body: Record<string, string>) => {
    try {
      const made = await request<TerminalInfo>("POST", `${base}/terminals`, body);
      setActive(made.group);
      setFocusGroup(made.group);
      reload();
    } catch (failure) {
      fail(failure);
    }
  };
  /** The engine's own CLI resumed on this thread's session; the service says when it can't. */
  const openConsole = async () => {
    try {
      const made = await request<TerminalInfo>("POST", `${base}/console`, {});
      setActive(made.group);
      setFocusGroup(made.group);
      reload();
    } catch (failure) {
      fail(failure);
    }
  };
  const close = async (terminal: TerminalInfo) => {
    try {
      await request("DELETE", `${base}/terminals/${q(terminal.id)}`);
      reload();
    } catch (failure) {
      fail(failure);
    }
  };
  /**
   * Where focus goes when a tab leaves: the neighbouring tab, else the "+". Remembered before the tab
   * goes (it leaves when the list is read again) and applied once the row has changed.
   */
  const focusBack = useRef<ReturnType<typeof captureFocusReturn> | null>(null);
  const rememberFocus = (group: string) => {
    const tabsNow = [...(root.current?.querySelectorAll<HTMLElement>(".term-head .ptab") ?? [])];
    const at = tabsNow.findIndex((tab) => tab.dataset.group === group);
    const neighbour = tabsNow[at + 1] ?? tabsNow[at - 1];
    focusBack.current = captureFocusReturn(neighbour?.querySelector<HTMLElement>("[role=tab]") ?? root.current?.querySelector<HTMLElement>(".padd"));
  };
  useLayoutEffect(() => {
    if (!focusBack.current) return;
    focusBack.current.restore();
    focusBack.current = null;
  }, [terminals.data, mainHidden]);
  /** A tab's X ends that shell; one that is alive asks first. The main shell's X only hides its tab here, so it asks nothing. */
  const end = (terminal: TerminalInfo) => {
    if (terminal.alive) setEnding(terminal);
    else {
      rememberFocus(terminal.group);
      void close(terminal);
    }
  };
  const hideMain = () => {
    rememberFocus(MAIN);
    setMainHidden(true);
    if (active === MAIN) setActive(tabs[0]?.[0]?.group ?? MAIN);
  };
  const run = async (script: ProjectScript) => {
    try {
      await runProjectAction(threadId, script);
    } catch (failure) {
      fail(failure);
    }
  };
  useCommand("terminal.new", () => void open({}));
  useCommand("terminal.close", () => {
    const first = shown?.[0];
    if (first && first.id !== MAIN) end(first);
    else onClose();
  });
  const scriptList = scripts.data?.scripts ?? [];
  const [tabMenu, setTabMenu] = useState<{ x: number; y: number; group: TerminalInfo[] } | null>(null);
  const currentTerminal = shown?.[0] && shown[0].id !== MAIN ? shown[0] : null;
  const running = currentTerminal !== null && currentTerminal.alive && currentTerminal.script !== null;
  const now = useNow(running);
  const keyOf = (terminal: TerminalInfo) => terminalKey(threadId, terminal.id === MAIN ? undefined : terminal.id);
  const keysOf = (group: TerminalInfo[]) => (group.length > 0 ? group.map(keyOf) : [terminalKey(threadId, undefined)]);
  const outputOf = (terminal: TerminalInfo) => controlsOf(keyOf(terminal))?.output() ?? "";
  const registered = (group: TerminalInfo[]) => keysOf(group).some((key) => controlsOf(key) !== undefined);
  const copyAll = (group: TerminalInfo[]) => {
    const text = keysOf(group).map((key) => controlsOf(key)?.output() ?? "").filter(Boolean).join("\n");
    if (!text) {
      pushToast("Nothing to copy");
      return;
    }
    navigator.clipboard?.writeText(text).then(
      () => pushToast("Output copied"),
      () => pushToast("Couldn't copy the output"),
    );
  };
  const sendToAgent = (terminal: TerminalInfo) => {
    const output = outputOf(terminal);
    if (!output) return;
    const lines = output.split("\n").length;
    if (addTerminalChip(threadId, { title: "Terminal output", output, lineStart: 1, lineEnd: lines, command: terminal.title, exitCode: terminal.exit_code }, "Terminal output exceeds the message limit. Select a smaller excerpt.")) {
      pushToast("Output added to your message");
    }
  };
  const runAgain = (terminal: TerminalInfo) => {
    const script = scriptList.find((one) => one.name === terminal.script);
    if (script) void run(script);
  };
  const menuFor = (group: TerminalInfo[]): MenuItem[] => {
    const first = group[0];
    const there = registered(group);
    return [
      ...(mainHidden ? [{ label: "Show Terminal", onSelect: () => { setMainHidden(false); setActive(MAIN); } }] : []),
      { label: "Clear output", disabled: !there, ...(mainHidden ? { separatorBefore: true } : {}), onSelect: () => keysOf(group).forEach((key) => controlsOf(key)?.clear()) },
      { label: "Copy all output", disabled: !there, onSelect: () => copyAll(group) },
      { label: "Watch only", selected: watching, onSelect: () => setWatching(!watching) },
      ...(first && first.id !== MAIN && first.alive
        ? [{ label: "End process", icon: <Square size={14} aria-hidden />, destructive: true, separatorBefore: true, onSelect: () => end(first) }]
        : []),
    ];
  };
  const onTabMenu = (event: MouseEvent, group: TerminalInfo[]) => {
    event.preventDefault();
    setTabMenu({ x: event.clientX, y: event.clientY, group });
  };
  const onTabKey = (event: KeyboardEvent, group: TerminalInfo[]) => {
    if (event.key !== "ContextMenu" && !(event.shiftKey && event.key === "F10")) return;
    event.preventDefault();
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    setTabMenu({ x: rect.left, y: rect.bottom, group });
  };
  const mainGroup = groups.find((group) => group[0]?.group === MAIN) ?? [];
  // The search key inside the drawer searches its output instead of reaching the shell: Cmd+F on a Mac
  // (Ctrl+F stays the shell's), Ctrl+F elsewhere. Pressed again while search is open, it returns to the field.
  const searchOpen = useRef(false);
  searchOpen.current = searching;
  useEffect(() => {
    const onKey = (event: globalThis.KeyboardEvent) => {
      if (event.key.toLowerCase() !== "f" || event.altKey || event.shiftKey) return;
      const mac = /mac/i.test(navigator.platform || navigator.userAgent);
      const accel = mac ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
      if (!accel) return;
      if (!(event.target instanceof Node) || !root.current?.contains(event.target)) return;
      event.preventDefault();
      event.stopPropagation();
      if (searchOpen.current) setSearchFocus((count) => count + 1);
      else setSearching(true);
    };
    document.addEventListener("keydown", onKey, true);
    return () => document.removeEventListener("keydown", onKey, true);
  }, []);
  // Closing the search puts focus on the terminal itself.
  const wasSearching = useRef(false);
  useLayoutEffect(() => {
    if (searching) wasSearching.current = true;
    else if (wasSearching.current) {
      wasSearching.current = false;
      root.current?.querySelector<HTMLElement>(".terminal-splits .xterm-helper-textarea")?.focus();
    }
  }, [searching]);
  const searchKey = keyOf(shown?.[0] ?? { id: MAIN } as TerminalInfo);
  return (
    <div className="terminal-drawer-body" ref={root}>
      <div className="terminal-tabs term-head" role="tablist" aria-label="Terminals">
        {mainHidden ? null : (
          <div className="ptab" data-group={MAIN} data-selected={current === MAIN || undefined} onContextMenu={(event) => onTabMenu(event, mainGroup)} onKeyDown={(event) => onTabKey(event, mainGroup)}>
            <button type="button" role="tab" aria-label="Terminal" aria-selected={current === MAIN} onClick={() => setActive(MAIN)}>
              <SquareTerminal size={14} aria-hidden />
              <span className="ptl">Terminal</span>
            </button>
            <button type="button" className="ptx" aria-label="Close Terminal" onClick={hideMain}>
              <X size={11} aria-hidden />
            </button>
          </div>
        )}
        {tabs.map((group) => {
          const first = group[0];
          if (!first) return null;
          const selected = first.group === current;
          const title = `${first.title}${group.length > 1 ? ` (${group.length})` : ""}`;
          return (
            <div key={first.group} className="ptab" data-group={first.group} data-selected={selected || undefined} onContextMenu={(event) => onTabMenu(event, group)} onKeyDown={(event) => onTabKey(event, group)}>
              <button type="button" role="tab" aria-label={title} aria-selected={selected} onClick={() => setActive(first.group)}>
                <SquareTerminal size={14} aria-hidden />
                <span className="ptl">
                  <Marker terminal={first} />
                  {title}
                </span>
              </button>
              <button type="button" className="ptx" aria-label={`Close ${first.title}`} onClick={() => end(first)}>
                <X size={11} aria-hidden />
              </button>
            </div>
          );
        })}
        <IconButton className="padd" label="New terminal" onClick={() => void open({})}>
          <Plus size={14} aria-hidden />
        </IconButton>
        <span className="terminal-tabs-gap" />
        <IconButton label="Watch only" pressed={watching} onClick={() => setWatching(!watching)}>
          <Eye size={14} aria-hidden />
        </IconButton>
        <IconButton label="Open engine console" onClick={() => void openConsole()}>
          <SquareTerminal size={14} aria-hidden />
        </IconButton>
        {scriptList.length > 0 ? (
          <IconButton
            label="Run a script"
            onClick={(event) => {
              const rect = event.currentTarget.getBoundingClientRect();
              setMenu({ x: rect.left, y: rect.top - 8 - scriptList.length * 30 });
            }}
          >
            <Play size={14} aria-hidden />
          </IconButton>
        ) : null}
        <IconButton label={maximized ? "Restore" : "Maximise terminal"} onClick={onToggleMaximize}>
          {maximized ? <Minimize2 size={14} aria-hidden /> : <Maximize2 size={14} aria-hidden />}
        </IconButton>
        <IconButton label="Collapse terminal" onClick={onClose}>
          <ChevronDown size={14} aria-hidden />
        </IconButton>
      </div>
      {searching && !empty ? <TerminalSearch keyOf={searchKey} focusSignal={searchFocus} onClose={() => setSearching(false)} /> : null}
      {empty ? (
        <div className="term-empty">
          <span>No terminals open.</span>
          <Button small onClick={() => void open({})}>
            New terminal
          </Button>
          {mainHidden ? (
            <Button small tone="quiet" onClick={() => { setMainHidden(false); setActive(MAIN); }}>
              Show Terminal
            </Button>
          ) : null}
        </div>
      ) : (
        <div className="terminal-splits" data-direction={shown?.[1]?.direction ?? undefined}>
          {shown ? (
            shown.map((terminal) => (
              <div key={`${terminal.id}:${terminal.created_at}`} className="term-pane">
                {shown.length > 1 ? (
                  <div className="term-pane-title">
                    {terminal.alive && terminal.script ? <i className="spin" aria-label="Working" /> : null}
                    <span>{terminal.id === MAIN ? "Terminal" : terminal.title}</span>
                  </div>
                ) : null}
                <TerminalPanel
                  threadId={threadId}
                  observe={watching}
                  autoFocus={terminal.group === focusGroup}
                  {...(terminal.id === MAIN ? {} : { terminalId: terminal.id })}
                />
              </div>
            ))
          ) : (
            <TerminalPanel threadId={threadId} observe={watching} />
          )}
        </div>
      )}
      {currentTerminal && running ? (
        <div className="term-dock" role="status">
          <i className="spin" aria-hidden />
          <span>Running · {elapsed(currentTerminal.created_at, now)}</span>
          <span className="term-dock-acts">
            <Button small disabled={controlsOf(keyOf(currentTerminal)) === undefined} onClick={() => controlsOf(keyOf(currentTerminal))?.clear()}>
              Clear
            </Button>
            {watching ? null : (
              <Button small onClick={() => controlsOf(keyOf(currentTerminal))?.interrupt()}>
                <Square size={12} aria-hidden />
                Stop
              </Button>
            )}
          </span>
        </div>
      ) : currentTerminal && !currentTerminal.alive && currentTerminal.exit_code !== null && currentTerminal.exit_code !== 0 ? (
        <div className="term-dock" data-tone="failed" role="status">
          <span className="term-failed">
            <X size={12} aria-hidden />
            Exited with code {currentTerminal.exit_code}
          </span>
          <span className="term-dock-title">{currentTerminal.title}</span>
          <span className="term-dock-acts">
            <Button small onClick={() => sendToAgent(currentTerminal)}>
              <MessageSquare size={12} aria-hidden />
              Send to the agent
            </Button>
            {currentTerminal.script && scriptList.some((one) => one.name === currentTerminal.script) ? (
              <Button small onClick={() => runAgain(currentTerminal)}>
                <RotateCw size={12} aria-hidden />
                Run again
              </Button>
            ) : null}
          </span>
        </div>
      ) : currentTerminal && !currentTerminal.alive ? (
        <div className="term-dock" role="status">
          <span>Ended</span>
          <span className="term-dock-acts">
            <Button small onClick={() => void close(currentTerminal)}>
              Close tab
            </Button>
          </span>
        </div>
      ) : null}
      <Presence>{menu ? (
        <Menu
          {...menu}
          label="Scripts"
          onClose={() => setMenu(null)}
          items={scriptList.map((script) => ({ label: script.name, icon: <ProjectActionIcon icon={script.icon} />, onSelect: () => void run(script) }))}
        />
      ) : null}</Presence>
      <Presence>{tabMenu ? <Menu x={tabMenu.x} y={tabMenu.y} label="Terminal tab" items={menuFor(tabMenu.group)} onClose={() => setTabMenu(null)} /> : null}</Presence>
      {ending ? (
        <ConfirmDialog title="End this terminal?" confirmLabel="End" tone="danger" cancelFocus onConfirm={async () => {
            rememberFocus(ending.group);
            await close(ending);
          }} onClose={() => setEnding(null)}>
          <p>{ending.title} is still running. Ending the terminal stops it.</p>
        </ConfirmDialog>
      ) : null}
    </div>
  );
}
