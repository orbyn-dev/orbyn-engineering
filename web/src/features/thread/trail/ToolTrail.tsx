import { ChevronRight, Eye, FileText, Folder, Globe, GitFork, Image as ImageIcon, Loader2, Minus, Plug, Search, Square, SquarePen, SquareTerminal, X, AppWindow, Wrench, type LucideIcon } from "lucide-react";
import { use, useEffect, useRef, useState } from "react";
import { callDuration } from "../../../lib/format";
import { callOutcome, traceTool, type CallOutcome } from "../../../product/timeline/toolTrace";
import type { ToolItem } from "../../../product/timeline/types";
import { useNow } from "../../../lib/useNow";
import { useThreadLogs } from "../../../product/state/threadLogs";
import { useWorkspace } from "../../../product/state/workspace";
import { Collapsible } from "../../../ui/Collapsible";
import { useOpenControl, useOpenState } from "../../../ui/openState";
import { HtmlRender } from "../HtmlRender";
import { McpApp } from "../McpApp";
import { SurfacedWorkflows, WorkflowCards } from "../WorkflowControls";
import { useThreadOfTimeline } from "../timelineThread";
import { callKind, callLabel, callMs, callState, runLabel, runOutcomes, type CallKind, type CallRun, type CallState } from "../toolCalls";
import { crewStartDiskLow } from "../../crew";
import { Block } from "./blocks/Block";
import { useRunRow } from "../../codemode";

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

/** The exit code a failed call adds after Failed: only the one the engine sent, never a made-up one. */
const failedExit = (item: ToolItem): string =>
  item.exitCode !== null && item.exitCode !== 0 ? ` · exit code ${item.exitCode}` : "";

/** The right side of a row as the design words it: the duration, or the outcome the engine did not report as success. */
function CallResult({ item, outcome, interrupted, ms }: { item: ToolItem; outcome: CallOutcome | null; interrupted: boolean; ms: number | null }) {
  if (outcome === "failed") {
    return (
      <span className="du">
        <span className="fail">Failed</span>
        {failedExit(item)}
      </span>
    );
  }
  if (outcome === "no-result") return <span className="du">No result reported · ended without one</span>;
  // An interrupted run names its cause; an older log or a lost state has none to name.
  if (outcome === "unknown") return <span className="du">{interrupted ? "Outcome unknown · run interrupted" : "Outcome unknown"}</span>;
  const text = ms === null ? null : callDuration(ms);
  return <span className="du">{text ?? ""}</span>;
}

/** The right side as plain text, for the row's spoken name. */
function resultText(item: ToolItem, outcome: CallOutcome | null, interrupted: boolean, ms: number | null): string {
  if (outcome === "failed") return `Failed${failedExit(item)}`;
  if (outcome === "no-result") return "No result reported · ended without one";
  if (outcome === "unknown") return interrupted ? "Outcome unknown · run interrupted" : "Outcome unknown";
  return ms === null ? "" : callDuration(ms);
}

/** The row's status for a reader who cannot see its mark: the label replaces the button's own content. */
function withState(label: string, state: CallState): string {
  // A label that already says "Running" does not say it twice.
  if (state === "running" && label.startsWith("Running")) return label;
  const word = state === "running" ? "running" : state === "failed" ? "failed" : state === "stopped" ? "stopped" : "";
  return word ? `${label}, ${word}` : label;
}

function StateIcon({ state, outcome = null, Icon, group = false }: { state: CallState; outcome?: CallOutcome | null; Icon: LucideIcon; group?: boolean }) {
  if (state === "running") return <Loader2 size={14} className="spin" role="img" aria-label="Working" />;
  if (outcome === "failed") return <X size={14} role="img" aria-label="Tool call failed" />;
  // A call the engine never answered keeps the quiet minus; only the row's words name the outcome.
  if (outcome === "no-result" || outcome === "unknown") return <Minus size={14} aria-hidden />;
  // A group keeps its kind icon; only a single failed call is marked with the cross.
  if (state === "failed" && !group) return <X size={14} role="img" aria-label="Tool call failed" />;
  if (state === "stopped") return group ? <Square size={14} role="img" aria-label="Stopped" /> : <Minus size={14} aria-hidden />;
  return <Icon size={14} aria-hidden />;
}

const stateClass = (state: CallState) => (state === "running" ? "s-run" : state === "failed" ? "s-fail" : state === "stopped" ? "s-stop" : "");
/** A group row is never drawn failed: its count says how many calls failed, and the failed call's own row is marked. */
const groupClass = (state: CallState) => (state === "failed" ? "" : stateClass(state));

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
  // A Code Mode run is drawn from its run, not from the call: it can be paused or failed while the call itself finished.
  const codeRow = useRunRow(item, live);
  const callStateNow = callState(item, live, cancelled);
  const state: CallState = codeRow ? (codeRow.tone === "run" ? "running" : codeRow.tone === "fail" ? "failed" : "done") : callStateNow;
  // What the engine reported for this call; only a call still running has no outcome to read yet.
  const outcome = codeRow || state === "running" ? null : callOutcome(item, cancelled);
  const running = callStateNow === "running";
  const now = useNow(running);
  const ms = callMs(item, running ? now : null);
  const kind = callKind(item);
  const label = codeRow ? { ...callLabel(item, cwd), note: codeRow.note } : callLabel(item, cwd);
  const lowDisk = crewStartDiskLow(item);
  const Icon = KIND_ICONS[kind];
  const full = [label.verb, label.object, label.note].filter(Boolean).join(" ");
  const { tip, label: labelRef, props } = useTip();
  // The visible right side is the spoken one, except a running row that must also say it is running.
  const spoken = codeRow
    ? withState([full, codeRow.due].filter(Boolean).join(" "), state)
    : state === "running"
      ? withState([full, resultText(item, null, cancelled, ms)].filter(Boolean).join(" "), state)
      : [full, resultText(item, outcome, cancelled, ms)].filter(Boolean).join(" ");
  const command = kind === "command" ? traceTool(item).command : null;
  return (
    <>
      <Collapsible
        className="tcw"
        persistId={`tool:${item.key}`}
        data-live={running || undefined}
        summaryClassName={`tcr ${codeRow?.tone === "wait" ? "s-wait" : stateClass(state)}`.trim()}
        summaryProps={{ ...props, "aria-label": spoken }}
        summary={
          <>
            <span className="tic">
              <StateIcon state={state} outcome={outcome} Icon={Icon} />
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
            {lowDisk ? <span className="crew-tool-state">Not started</span> : null}
            {codeRow ? (
              <span className="du">{codeRow.tone === "wait" ? <span className="am">{codeRow.due}</span> : codeRow.due}</span>
            ) : (
              <CallResult item={item} outcome={outcome} interrupted={cancelled} ms={ms} />
            )}
            <span className="tcv">
              <ChevronRight size={14} aria-hidden />
            </span>
          </>
        }
      >
        <Block item={item} running={running} state={callStateNow} ms={ms} cwd={cwd} live={live} />
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
  const lead = !open && running && current ? "Running" : label.verb;
  // Only a call still running is named on a collapsed row; a failed call never replaces the count,
  // so the row keeps "Ran 6 commands" and says how many of them did not succeed. The open list names each.
  const detail = !open && running && current ? callLabel(current, cwd).object : null;
  const outcomes = detail ? null : runOutcomes(run, live, cancelled);
  const duration = total === null ? "" : callDuration(total);
  const command = detail && run.kind === "command" ? detail : undefined;
  return (
    <Collapsible
      open={open}
      onOpenChange={setOpen}
      className="tcw"
      summaryClassName={`tcr ${groupClass(state)}`.trim()}
      summaryProps={{
        "aria-label": withState(
          [lead, detail, outcomes ? `· ${outcomes.text}` : null, !detail && label.note ? `· ${label.note}` : null, duration].filter(Boolean).join(" "),
          state,
        ),
      }}
      summary={
        <>
          <span className="tic">
            <StateIcon state={state} Icon={Icon} group />
          </span>
          <span className="lb" data-command={command}>
            <span className="tla">
              {lead}
              {outcomes ? (
                <>
                  {" · "}
                  <span className={outcomes.failed ? "tla-fail" : undefined}>{outcomes.text}</span>
                </>
              ) : null}
            </span>
            {detail ? <span className="tlb tlfill">{detail}</span> : null}
            {!detail && label.note ? <span className="tln">· {label.note}</span> : null}
          </span>
          <span className="du">{duration}</span>
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
