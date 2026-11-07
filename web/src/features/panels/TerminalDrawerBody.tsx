import { useCommand } from "../../app/shortcuts";
import { Check, Columns2, Eye, ChevronUp, MessageSquare, Play, Plus, RotateCw, Rows2, Square, SquareTerminal, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useState, type MouseEvent } from "react";
import { request } from "../../api/client";
import type { ProjectFileView, TerminalInfo } from "../../api/types";
import { elapsed } from "../../lib/format";
import { toFailure, useAsync } from "../../lib/useAsync";
import { useNow } from "../../lib/useNow";
import { useUi } from "../../state/ui";
import { Button, IconButton } from "../../ui/controls";
import { Menu, type MenuItem } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";
import { TerminalPanel } from "./TerminalPanel";
import { controlsOf, terminalKey } from "./terminalControls";
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

/** The slim bar left under the thread when the drawer is closed and something is still running. */
export function TerminalBar({ threadId, onOpen }: { threadId: string; onOpen(): void }) {
  const terminals = useAsync(`terminal-bar:${threadId}`, (signal) =>
    request<TerminalInfo[]>("GET", `/api/threads/${q(threadId)}/terminals`, undefined, signal),
  );
  const { reload } = terminals;
  useEffect(() => {
    const timer = setInterval(reload, 10_000);
    return () => clearInterval(timer);
  }, [reload]);
  const count = runningCount(terminals.data);
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

/** Terminal tabs, each a group of splits, and the project's scripts. */
export function TerminalDrawerBody({ threadId, onClose }: { threadId: string; onClose(): void }) {
  const base = `/api/threads/${q(threadId)}`;
  const terminals = useAsync(`terminals:${threadId}`, (signal) =>
    request<TerminalInfo[]>("GET", `${base}/terminals`, undefined, signal),
  );
  const scripts = useAsync(`scripts:${threadId}`, (signal) =>
    request<ProjectFileView>("GET", `${base}/scripts`, undefined, signal),
  );
  const [active, setActive] = useState(MAIN);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [watching, setWatching] = useState(false);
  const pushToast = useUi((state) => state.pushToast);
  const { reload } = terminals;
  const reveal = useUi((state) => state.terminalReveal);
  useEffect(() => {
    if (reveal?.threadId !== threadId) return;
    setActive(reveal.group); reload();
  }, [reveal, threadId, reload]);
  const groups = useMemo(() => groupTerminals(terminals.data ?? []), [terminals.data]);
  const shown = groups.find((group) => group[0]?.group === active);
  const tabs = groups.filter((group) => group[0]?.group !== MAIN);
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
  const run = async (script: ProjectScript) => {
    try {
      await runProjectAction(threadId, script);
    } catch (failure) {
      fail(failure);
    }
  };
  const split = (direction: "horizontal" | "vertical", from = shown) => void open({ split_of: from?.[0]?.id ?? MAIN, direction });
  useCommand("terminal.split", () => split("horizontal"));
  useCommand("terminal.splitVertical", () => split("vertical"));
  useCommand("terminal.new", () => void open({}));
  useCommand("terminal.close", () => {
    const first = shown?.[0];
    if (first && first.id !== MAIN) void close(first);
    else onClose();
  });
  const scriptList = scripts.data?.scripts ?? [];
  const [tabMenu, setTabMenu] = useState<{ x: number; y: number; group: TerminalInfo[] } | null>(null);
  const current = shown?.[0] && shown[0].id !== MAIN ? shown[0] : null;
  const running = current !== null && current.alive && current.script !== null;
  const now = useNow(running);
  const keyOf = (terminal: TerminalInfo) => terminalKey(threadId, terminal.id === MAIN ? undefined : terminal.id);
  const outputOf = (terminal: TerminalInfo) => controlsOf(keyOf(terminal))?.output() ?? "";
  const copyAll = (group: TerminalInfo[]) => {
    const text = group.map(outputOf).filter(Boolean).join("\n");
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
    const first = group[0]!;
    return [
      { label: "Split right", icon: <Columns2 size={14} aria-hidden />, onSelect: () => split("horizontal", group) },
      { label: "Split down", icon: <Rows2 size={14} aria-hidden />, onSelect: () => split("vertical", group) },
      { label: "Clear output", separatorBefore: true, onSelect: () => group.forEach((terminal) => controlsOf(keyOf(terminal))?.clear()) },
      { label: "Copy all output", onSelect: () => copyAll(group) },
      { label: "Watch only", selected: watching, onSelect: () => setWatching(!watching) },
      ...(first.id !== MAIN && first.alive
        ? [{ label: "End process", icon: <Square size={14} aria-hidden />, destructive: true, separatorBefore: true, onSelect: () => void close(first) }]
        : []),
    ];
  };
  const onTabMenu = (event: MouseEvent, group: TerminalInfo[]) => {
    event.preventDefault();
    setTabMenu({ x: event.clientX, y: event.clientY, group });
  };
  const mainGroup = groups.find((group) => group[0]?.group === MAIN) ?? [];
  return (
    <div className="terminal-drawer-body">
      <div className="terminal-tabs term-head" role="tablist" aria-label="Terminals">
        <div className="ptab" data-selected={active === MAIN || undefined} onContextMenu={(event) => onTabMenu(event, mainGroup)}>
          <button type="button" role="tab" aria-selected={active === MAIN} onClick={() => setActive(MAIN)}>
            <SquareTerminal size={14} aria-hidden />
            <span className="ptl">Terminal</span>
          </button>
        </div>
        {tabs.map((group) => {
          const first = group[0];
          if (!first) return null;
          const selected = first.group === active;
          const title = `${first.title}${group.length > 1 ? ` (${group.length})` : ""}`;
          return (
            <div key={first.group} className="ptab terminal-tab" data-selected={selected || undefined} onContextMenu={(event) => onTabMenu(event, group)}>
              <button type="button" role="tab" aria-selected={selected} onClick={() => setActive(first.group)}>
                <SquareTerminal size={14} aria-hidden />
                <span className="ptl">
                  <Marker terminal={first} />
                  {title}
                </span>
              </button>
              <button type="button" className="ptx" aria-label={`Close ${first.title}`} onClick={() => void close(first)}>
                <X size={11} aria-hidden />
              </button>
            </div>
          );
        })}
        <span className="terminal-tabs-gap" />
        {terminals.data ? (
          <>
            <IconButton label="Split terminal horizontally" onClick={() => split("horizontal")}>
              <Columns2 size={14} aria-hidden />
            </IconButton>
            <IconButton label="Split terminal vertically" onClick={() => split("vertical")}>
              <Rows2 size={14} aria-hidden />
            </IconButton>
          </>
        ) : null}
        <IconButton label="Watch only" pressed={watching} onClick={() => setWatching(!watching)}>
          <Eye size={14} aria-hidden />
        </IconButton>
        <IconButton label="New terminal" onClick={() => void open({})}>
          <Plus size={14} aria-hidden />
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
        <IconButton label="Close terminal" onClick={onClose}>
          <X size={14} aria-hidden />
        </IconButton>
      </div>
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
                {...(terminal.id === MAIN ? {} : { terminalId: terminal.id })}
              />
            </div>
          ))
        ) : (
          <TerminalPanel threadId={threadId} observe={watching} />
        )}
      </div>
      {current && running ? (
        <div className="term-dock" role="status">
          <i className="spin" aria-hidden />
          <span>Running · {elapsed(current.created_at, now)}</span>
          <span className="term-dock-acts">
            <Button small onClick={() => controlsOf(keyOf(current))?.clear()}>
              Clear
            </Button>
            <Button small onClick={() => controlsOf(keyOf(current))?.interrupt()}>
              <Square size={12} aria-hidden />
              Stop
            </Button>
          </span>
        </div>
      ) : current && !current.alive && current.exit_code !== null && current.exit_code !== 0 ? (
        <div className="term-dock" data-tone="failed" role="status">
          <span className="term-failed">
            <X size={12} aria-hidden />
            Exited with code {current.exit_code}
          </span>
          <span className="term-dock-title">{current.title}</span>
          <span className="term-dock-acts">
            <Button small onClick={() => sendToAgent(current)}>
              <MessageSquare size={12} aria-hidden />
              Send to the agent
            </Button>
            {current.script && scriptList.some((one) => one.name === current.script) ? (
              <Button small onClick={() => runAgain(current)}>
                <RotateCw size={12} aria-hidden />
                Run again
              </Button>
            ) : null}
          </span>
        </div>
      ) : current && !current.alive ? (
        <div className="term-dock" role="status">
          <span>Ended</span>
          <span className="term-dock-acts">
            <Button small onClick={() => void close(current)}>
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
    </div>
  );
}
