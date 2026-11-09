import { ChevronRight, Eye, FileText, Folder, Globe, GitFork, Image as ImageIcon, Loader2, Plug, Search, Square, SquarePen, SquareTerminal, X, AppWindow, Wrench, type LucideIcon } from "lucide-react";
import { use, useEffect, useRef, useState } from "react";
import { callDuration } from "../../../lib/format";
import { traceTool } from "../../../lib/timeline/toolTrace";
import type { ToolItem } from "../../../lib/timeline/types";
import { useNow } from "../../../lib/useNow";
import { useThreadLogs } from "../../../state/threadLogs";
import { useWorkspace } from "../../../state/workspace";
import { Collapsible } from "../../../ui/Collapsible";
import { useOpenControl, useOpenState } from "../../../ui/openState";
import { HtmlRender } from "../HtmlRender";
import { McpApp } from "../McpApp";
import { SurfacedWorkflows, WorkflowCards } from "../WorkflowControls";
import { useThreadOfTimeline } from "../timelineThread";
import { callKind, callLabel, callMs, callState, runLabel, type CallKind, type CallRun, type CallState } from "../toolCalls";
import { Block } from "./blocks/Block";

const KIND_ICONS: Record<CallKind, LucideIcon> = {
  command: SquareTerminal,
  read: Eye,
  edit: SquarePen,
  create: FileText,
  search: Search,
  list: Folder,
  fetch: Globe,
  mcp: Plug,
  subagent: GitFork,
  image: ImageIcon,
  web: Globe,
  browser: AppWindow,
  other: Wrench,
};

function Duration({ ms, state, item }: { ms: number | null; state: CallState; item: ToolItem }) {
  const text = ms === null ? null : callDuration(ms);
  const exit = item.exitCode;
  if (state === "failed" && exit !== null && exit !== 0) {
    return (
      <span className="du">
        <span className="er">exit {exit}</span>
        {text ? ` · ${text}` : ""}
      </span>
    );
  }
  return <span className="du">{text ?? ""}</span>;
}

/** The row's status for a reader who cannot see its mark: the label replaces the button's own content. */
function withState(label: string, state: CallState): string {
  // A label that already says "Running" does not say it twice.
  if (state === "running" && label.startsWith("Running")) return label;
  const word = state === "running" ? "running" : state === "failed" ? "failed" : state === "stopped" ? "stopped" : "";
  return word ? `${label}, ${word}` : label;
}

/** The duration as it is read aloud: with the exit code when there is one. */
function spokenDuration(ms: number | null, state: CallState, item: ToolItem): string {
  const text = ms === null ? "" : callDuration(ms);
  if (state === "failed" && item.exitCode !== null && item.exitCode !== 0) return `exit ${item.exitCode}${text ? ` · ${text}` : ""}`;
  return text;
}

function StateIcon({ state, Icon }: { state: CallState; Icon: LucideIcon }) {
  if (state === "running") return <Loader2 size={14} className="spin" role="img" aria-label="Working" />;
  if (state === "failed") return <X size={14} role="img" aria-label="Tool call failed" />;
  if (state === "stopped") return <Square size={14} role="img" aria-label="Stopped" />;
  return <Icon size={14} aria-hidden />;
}

const stateClass = (state: CallState) => (state === "running" ? "s-run" : state === "failed" ? "s-fail" : state === "stopped" ? "s-stop" : "");

/** Shows the full text under a row whose label was cut off, while the row has the pointer or focus. */
function useTip() {
  const [tip, setTip] = useState(false);
  const label = useRef<HTMLSpanElement>(null);
  const show = () => {
    const element = label.current;
    if (element && element.scrollWidth > element.clientWidth) setTip(true);
  };
  return { tip, label, props: { onMouseEnter: show, onFocus: show, onMouseLeave: () => setTip(false), onBlur: () => setTip(false) } };
}

/** One tool call: a row, and the block it opens. */
export function ToolCallRow({ item, live }: { item: ToolItem; live: boolean }) {
  const threadId = useThreadOfTimeline();
  const cwd = useWorkspace((s) => (threadId ? s.threads[threadId]?.cwd ?? null : null));
  const cancelled = useThreadLogs((s) => (threadId && item.runId ? s.logs[threadId]?.runs[item.runId]?.outcome === "cancelled" : false));
  const surfaced = use(SurfacedWorkflows);
  const state = callState(item, live, cancelled);
  const running = state === "running";
  const now = useNow(running);
  const ms = callMs(item, running ? now : null);
  const kind = callKind(item);
  const label = callLabel(item, cwd);
  const Icon = KIND_ICONS[kind];
  const full = [label.verb, label.object, label.note].filter(Boolean).join(" ");
  const { tip, label: labelRef, props } = useTip();
  const spoken = withState([full, spokenDuration(ms, state, item)].filter(Boolean).join(" "), state);
  const command = kind === "command" ? traceTool(item).command : null;
  return (
    <>
      <Collapsible
        className="tcw"
        persistId={`tool:${item.key}`}
        data-live={running || undefined}
        summaryClassName={`tcr ${stateClass(state)}`.trim()}
        summaryProps={{ ...props, "aria-label": spoken }}
        summary={
          <>
            <span className="tic">
              <StateIcon state={state} Icon={Icon} />
            </span>
            <span className="lb" ref={labelRef} data-command={command ?? undefined}>
              {label.verb ? <span className="tla">{label.verb}</span> : null}
              {label.object ? <span className={`tlb${label.fill ? " tlfill" : ""}`}>{label.object}</span> : null}
              {label.note ? <span className="tln">{label.note}</span> : null}
              {tip ? (
                <span className="tctip" role="tooltip">
                  {full}
                </span>
              ) : null}
            </span>
            <Duration ms={ms} state={state} item={item} />
            <span className="tcv">
              <ChevronRight size={14} aria-hidden />
            </span>
          </>
        }
      >
        <Block item={item} running={running} state={state} ms={ms} cwd={cwd} />
      </Collapsible>
      {/* A published page is the result, so it stays visible with the details closed. */}
      <HtmlRender item={item} threadId={threadId} />
      <McpApp item={item} threadId={threadId} />
      {surfaced ? null : <WorkflowCards items={[item]} threadId={threadId} live={live} />}
    </>
  );
}

/** Consecutive calls of one kind: a header, then the calls one indent in. One call is just its own row. */
export function TrailRun({ run, live }: { run: CallRun; live: boolean }) {
  const [open, setOpen] = useOpenState(`tg:${run.key}`, false);
  const { peek } = useOpenControl();
  const threadId = useThreadOfTimeline();
  const cancelled = useThreadLogs((s) => (threadId && run.items[0]?.runId ? s.logs[threadId]?.runs[run.items[0].runId]?.outcome === "cancelled" : false));
  const only = run.items[0];
  const states = run.items.map((item) => callState(item, live, cancelled));
  const running = states.includes("running");
  const failedAt = states.indexOf("failed");
  const state: CallState = running ? "running" : failedAt >= 0 ? "failed" : states.includes("stopped") ? "stopped" : "done";
  const now = useNow(running);
  const total = run.items.reduce<number | null>((sum, item) => {
    const ms = callMs(item, callState(item, live, cancelled) === "running" ? now : null);
    return ms === null ? sum : (sum ?? 0) + ms;
  }, null);
  const cwd = useWorkspace((s) => (threadId ? s.threads[threadId]?.cwd ?? null : null));
  // When a second call joins a row someone opened, the new group opens with it rather than hiding it.
  // Keyed off what the thread remembers, not what is mounted: a group nobody has opened or closed yet.
  const count = run.items.length;
  useEffect(() => {
    if (count >= 2 && peek(`tg:${run.key}`) === undefined && run.items.some((item) => peek(`tool:${item.key}`) === true)) setOpen(true);
  }, [count, run.key, run.items, peek, setOpen]);
  if (!only) return null;
  if (count === 1) return <ToolCallRow item={only} live={live} />;
  const label = runLabel(run, running);
  const Icon = KIND_ICONS[run.kind];
  const current = run.items[states.lastIndexOf("running")];
  const failing = failedAt >= 0 ? run.items[failedAt] : undefined;
  const lead = !open && running && current ? "Running" : label.verb;
  const detail = !open ? (running && current ? callLabel(current, cwd).object : failing ? callLabel(failing, cwd).object : null) : null;
  const exit = failing?.exitCode;
  const exited = !open && failing && exit !== null && exit !== undefined && exit !== 0;
  const duration = total === null ? "" : callDuration(total);
  const command = detail && run.kind === "command" ? detail : undefined;
  return (
    <Collapsible
      open={open}
      onOpenChange={setOpen}
      className="tcw"
      summaryClassName={`tcr ${stateClass(state)}`.trim()}
      summaryProps={{
        "aria-label": withState(
          [lead, detail, !detail && label.note ? `· ${label.note}` : null, exited ? `exit ${exit} ·` : null, duration].filter(Boolean).join(" "),
          state,
        ),
      }}
      summary={
        <>
          <span className="tic">
            <StateIcon state={state} Icon={Icon} />
          </span>
          <span className="lb" data-command={command}>
            <span className="tla">{lead}</span>
            {detail ? <span className="tlb tlfill">{detail}</span> : null}
            {!detail && label.note ? <span className="tln">· {label.note}</span> : null}
          </span>
          <span className="du">
            {exited ? (
              <>
                <span className="er">exit {exit}</span> · {duration}
              </>
            ) : (
              duration
            )}
          </span>
          <span className="tcv">
            <ChevronRight size={14} aria-hidden />
          </span>
        </>
      }
    >
      <div className="tk">
        {run.items.map((item) => (
          <ToolCallRow key={item.key} item={item} live={live} />
        ))}
      </div>
    </Collapsible>
  );
}

/** Rows a person moves between with the arrow keys. */
export function trailKeys(event: React.KeyboardEvent<HTMLElement>, onCopied?: () => void) {
  const target = event.target as HTMLElement;
  if (!target.matches("button.tcr")) return;
  const root = event.currentTarget;
  const rows = [...root.querySelectorAll<HTMLButtonElement>("button.tcr")].filter((row) => !row.closest("[hidden]"));
  const at = rows.indexOf(target as HTMLButtonElement);
  const open = target.getAttribute("aria-expanded") === "true";
  switch (event.key) {
    case "ArrowDown":
      event.preventDefault();
      rows[Math.min(at + 1, rows.length - 1)]?.focus();
      break;
    case "ArrowUp":
      event.preventDefault();
      rows[Math.max(at - 1, 0)]?.focus();
      break;
    case "ArrowRight":
      if (!open) {
        event.preventDefault();
        target.click();
      }
      break;
    case "ArrowLeft":
      if (open) {
        event.preventDefault();
        target.click();
      }
      break;
    case "c":
    case "C": {
      if (event.metaKey || event.ctrlKey || event.altKey) break;
      const command = target.querySelector<HTMLElement>("[data-command]")?.dataset.command;
      if (command) {
        event.preventDefault();
        void navigator.clipboard?.writeText(command).then(() => onCopied?.());
      }
      break;
    }
  }
}
