import { PreparationRetryButton, ResendButton } from "./ThreadBanners";
import { useWorkspace } from "../../product/state/workspace";
import { MessageActions } from "./MessageActions";
import { failureWantsResend } from "./runFailures";
import {
  Brain,
  Check,
  ChevronRight,
  Loader2,
  Clock,
  FileDiff as FileDiffIcon,
  OctagonAlert,
  Terminal,
  Users,
} from "lucide-react";
import { memo, useMemo, useState } from "react";
import { Square } from "lucide-react";
import { clockTime, elapsed } from "../../lib/format";
import { useNow } from "../../lib/useNow";
import { EngineGlyph } from "../../product/ui/badges";
import { Collapsible } from "../../ui/Collapsible";
import { OpenStatePrefix, useOpenControl, useOpenState } from "../../ui/openState";
import { useUi } from "../../product/state/ui";
import { Button, Pill } from "../../ui/controls";
import { ApprovalCard, useResolvedApproval } from "./ApprovalCard";
import { SurfacedWorkflows, WorkflowCards } from "./WorkflowControls";
import { Markdown } from "../content";
import { PlanCard } from "./PlanCard";
import { proposedPlanIn, textAroundPlan } from "./proposedPlan";
import { SentFiles } from "./SentFiles";
import {
  type TimelineEntry,
  type TurnFold,
  type WorkGroup,
  type WorkingRow,
  type WorkItem,
} from "../../product/timeline/timeline";
import { useThreadLogs } from "../../product/state/threadLogs";
import { turnOutcomeText, workSegments } from "./toolCalls";
import { PlanRow } from "./trail/PlanRow";
import { TrailRun, trailKeys } from "./trail/ToolTrail";
import { useThreadOfTimeline } from "./timelineThread";
import { ClaudeHostActions, useHostFor } from "../enginehost";
import { isClaudeEngine, useClaudeEnginePath } from "../../product/engines/claudePath";
import type { ActivityItem, ApprovalItem, DiffSummaryItem, MessageItem, NoticeItem, ReasoningItem, StoppedItem, StoppedTask, ToolItem } from "../../product/timeline/types";

const STEER_MARKS = {
  pending: { text: "Sending…", icon: <Loader2 size={13} className="spin" aria-hidden /> },
  delivered: { text: "Delivered", icon: <Check size={13} className="steer-ok" aria-hidden /> },
  queued: { text: "Queued for the next turn", icon: <Clock size={13} aria-hidden /> },
  failed: { text: "Not delivered · moved to the queue", icon: <OctagonAlert size={13} aria-hidden /> },
} as const;

/** Where a mid-turn message went, under its bubble. */
function SteerMark({ state }: { state: NonNullable<MessageItem["steer"]> }) {
  const mark = STEER_MARKS[state];
  return (
    <span className={`steer-mark${state === "failed" ? " failed" : ""}`} role="status" data-state={state}>
      {mark.icon}
      {mark.text}
    </span>
  );
}

function Message({ item, live }: { item: MessageItem; live: boolean }) {
  if (item.role === "user") {
    return (
      <div className="msg-user" data-minimap={item.key}>
        <span className="sr-only">You said:</span>
        <p className="msg-bubble">{item.text}</p>
        <SentFiles attachments={item.attachments} text={item.text} />
        {item.steer ? <SteerMark state={item.steer} /> : null}
        <MessageActions text={item.text} runId={item.runId} role="user" />
      </div>
    );
  }
  const plan = item.done ? proposedPlanIn(item.text) : null;
  return (
    <div className="msg-assistant">
      {plan ? (
        <>
          {textAroundPlan(item.text) ? <Markdown text={textAroundPlan(item.text)} /> : null}
          <PlanCard text={plan} />
        </>
      ) : (
        <Markdown text={item.text} />
      )}
      {live && !item.done ? <span className="caret" aria-hidden /> : null}
      {item.done ? <MessageActions text={item.text} runId={item.runId} role="assistant" /> : null}
    </div>
  );
}

function Reasoning({ item, live }: { item: ReasoningItem; live: boolean }) {
  const streaming = live && !item.done;
  return (
    <Collapsible
      className="trace"
      persistId={`think:${item.key}`}
      summaryClassName="trace-summary"
      summary={
        <>
          <Brain size={14} aria-hidden />
          <span className="trace-label">
            {streaming ? "Thinking" : "Thought"}
          </span>
          <ChevronRight size={14} className="chev trace-end" aria-hidden />
        </>
      }
    >
      <div className="trace-body trace-prose">{item.text}</div>
    </Collapsible>
  );
}

function ApprovalEntry({ item }: { item: ApprovalItem }) {
  const approval = useResolvedApproval(item.approval);
  return <ApprovalCard approval={approval} minimapKey={item.key} />;
}

function ChangedFile({ file }: { file: DiffSummaryItem["files"][number] }) {
  const revealDiff = useUi((state) => state.revealDiff);
  const threadId = useThreadOfTimeline();
  return (
    <li>
      <button type="button" onClick={() => {
        const thread = threadId ? useWorkspace.getState().threads[threadId] : null;
        if (thread) revealDiff(file.path, { threadId: thread.id, cwd: thread.cwd });
      }}>
        <span title={file.path}>{file.path}</span>
        {file.additions !== null ? <span className="diff-add">+{file.additions}</span> : null}
        {file.deletions !== null ? <span className="diff-del">-{file.deletions}</span> : null}
      </button>
    </li>
  );
}

function DiffSummary({ item }: { item: DiffSummaryItem }) {
  const openPanel = useUi((state) => state.togglePanel);
  const panelShowing = useUi((state) => state.panelOpen === true && state.panelTab === "diff");
  const [open, setOpen] = useOpenState(`diff:${item.key}`, false);
  const added = item.files.reduce((sum, file) => sum + (file.additions ?? 0), 0);
  const removed = item.files.reduce((sum, file) => sum + (file.deletions ?? 0), 0);
  return (
    <div className="diff-card">
      <div className="diff-summary">
        <FileDiffIcon size={16} aria-hidden />
        <button type="button" className="diff-count" aria-expanded={open} onClick={() => setOpen(!open)}>
          {item.files.length === 1 ? "1 file changed" : `${item.files.length} files changed`}
        </button>
        <span className="diff-add">+{added}</span>
        <span className="diff-del">-{removed}</span>
        <Button
          small
          tone="quiet"
          onClick={() => {
            if (!panelShowing) openPanel("diff");
          }}
        >
          View diff
        </Button>
      </div>
      {open ? (
        <ul className="changed-files" aria-label="Changed files">
          {item.files.map((file) => (
            <ChangedFile key={file.path} file={file} />
          ))}
        </ul>
      ) : null}
    </div>
  );
}

function Activity({ item }: { item: ActivityItem }) {
  return (
    <Collapsible
      className="trace"
      persistId={`activity:${item.key}`}
      summaryClassName="trace-summary"
      summary={
        <>
          <ChevronRight size={14} className="chev" aria-hidden />
          <span className="trace-label">{item.activityType}</span>
        </>
      }
    >
      <div className="trace-body">
        <pre tabIndex={0}>{JSON.stringify(item.content, null, 2)}</pre>
      </div>
    </Collapsible>
  );
}

/** The two run failures a Claude host owns: one that could not start, and one the host cut mid-run. */
const CLAUDE_HOST_FAILURES = new Set(["engine_unavailable", "engine_host_stopped"]);

/** The Claude host's two ways out, with the failed run itself and without sending anything. */
function ClaudeHostFailureActions({ threadId, code }: { threadId: string; code: string | null }) {
  const thread = useWorkspace((state) => state.threads[threadId]);
  const engine = useWorkspace((state) => (thread ? state.engines.data?.find((entry) => entry.id === thread.engine) : undefined));
  const { path } = useClaudeEnginePath();
  const hostFor = engine?.instance_of ?? thread?.engine ?? null;
  const host = useHostFor(hostFor);
  const [error, setError] = useState<string | null>(null);
  if (!thread || !isClaudeEngine(thread.engine, engine?.instance_of) || path !== "sdk" || !CLAUDE_HOST_FAILURES.has(code ?? "")) return null;
  return (
    <div className="notice-actions">
      <ClaudeHostActions host={host} hostId={host?.id ?? hostFor} onError={setError} showUpdate={false} alwaysRetry />
      {error ? <span className="field-error" role="alert">{error}</span> : null}
    </div>
  );
}

function Notice({ item }: { item: NoticeItem }) {
  const threadId = useThreadOfTimeline();
  const timestampFormat = useUi((state) => state.timestampFormat);
  const thread = useWorkspace((state) => (threadId ? state.threads[threadId] : undefined));
  const engine = useWorkspace((state) => (thread ? state.engines.data?.find((entry) => entry.id === thread.engine) : undefined));
  const preparation = item.code === "workspace_preparation_failed" || item.code === "workspace_preparation_unavailable";
  // A failure with nothing to retry sends the message again instead.
  const resendable = failureWantsResend(item.text);
  const hostFailure = !!threadId && CLAUDE_HOST_FAILURES.has(item.code ?? "") && !!thread && isClaudeEngine(thread.engine, engine?.instance_of);
  if (item.code === "run_stopped") {
    return (
      <div className="stop-mark" data-minimap={item.key}>
        <span aria-hidden />
        <Square size={12} aria-hidden />
        <span>
          {item.text}
          <span data-find-skip> · {clockTime(item.ts, timestampFormat)}</span>
        </span>
        <span aria-hidden />
      </div>
    );
  }
  if (item.tone === "neutral") {
    return <p className="notice">{item.text}</p>;
  }
  return (
    <div className="notice-error" role="alert" data-minimap={item.key}>
      <OctagonAlert size={16} aria-hidden />
      <div>
        <strong>Run failed</strong>
        <p>{item.text}</p>
        {item.code && !preparation && !resendable && !hostFailure ? <Pill>{item.code}</Pill> : null}
        {resendable && threadId ? (
          <div className="notice-actions">
            <ResendButton threadId={threadId} seq={item.seq} />
          </div>
        ) : null}
        {hostFailure && threadId ? <ClaudeHostFailureActions threadId={threadId} code={item.code} /> : null}
        {threadId && item.runId && item.code === "workspace_preparation_failed"
          ? <PreparationRetryButton threadId={threadId} runId={item.runId} /> : null}
      </div>
    </div>
  );
}

/** The kind of a stopped task: a Claude or Codex subagent, a shell, or any other background task. */
export function stoppedKind(taskType: string): "Subagent" | "Shell" | "Background task" {
  if (taskType === "local_agent" || taskType === "codex_subagent") return "Subagent";
  return taskType === "local_bash" ? "Shell" : "Background task";
}

/** One task the engine stopped when its run ended: its kind, and its own words for it. */
function StoppedRow({ task }: { task: StoppedTask }) {
  const kind = stoppedKind(task.taskType);
  const Icon = kind === "Subagent" ? Users : kind === "Shell" ? Terminal : Clock;
  return (
    <div className="stopped-row">
      <Icon size={14} aria-hidden />
      <span className="stopped-text">
        <span className="stopped-kind">{kind}</span>
        {task.description ? ` · ${task.description}` : null}
      </span>
    </div>
  );
}

/** The tasks the engine stopped when this run ended, one row each, in the order it reported them. */
function Stopped({ item }: { item: StoppedItem }) {
  return (
    <div className="stopped" data-minimap={item.key}>
      <div className="stop-mark">
        <span aria-hidden />
        <Square size={12} aria-hidden />
        <span>Stopped when the run ended</span>
        <span aria-hidden />
      </div>
      <section className="steps tc" aria-label="Stopped when the run ended">
        {item.tasks.map((task) => (
          <StoppedRow key={task.taskId} task={task} />
        ))}
      </section>
    </div>
  );
}

function RunningStack({ tools }: { tools: ToolItem[] }) {
  const now = useNow(true);
  const oldest = tools[0];
  const shown = tools.slice(0, 3);
  return (
    <div className="tool-stack">
      <span className="tool-avatars" aria-hidden>
        {shown.map((tool) => (
          <EngineGlyph key={tool.key} name={tool.name} />
        ))}
        {tools.length > shown.length ? <span className="tool-more">+{tools.length - shown.length}</span> : null}
      </span>
      <span className="tool-stack-text">
        <strong>{tools.length} tools</strong>
        <span className="tone-info">{tools.length} working</span>
      </span>
      {oldest ? <span className="tool-stack-time">{elapsed(oldest.ts, now)}</span> : null}
    </div>
  );
}

/** The key hint is for keyboard focus; a pointer press on a row must not shift what lies below it. */
function keyboardOnRow(target: HTMLElement): boolean {
  if (!target.matches("button.tcr")) return false;
  try {
    return target.matches(":focus-visible");
  } catch {
    return true;
  }
}

/** Tool calls read as rows by kind; reasoning sits between them where it happened. */
export function WorkGroupView({ group, live }: { group: WorkGroup; live: boolean }) {
  const threadId = useThreadOfTimeline();
  const [focus, setFocus] = useState<{ copy: boolean } | null>(null);
  const [copied, setCopied] = useState(false);
  // Reasoning stands where it happened; the calls between are read as rows by kind, once per change.
  const segments = useMemo(() => workSegments(group.items), [group.items]);
  const several = group.items.length > 1;
  const rows = segments.map((segment) =>
    "reasoning" in segment ? (
      <Reasoning key={segment.reasoning.key} item={segment.reasoning} live={live} />
    ) : (
      segment.runs.map((run) => (
        <div className="tg" key={run.key}>
          <TrailRun run={run} live={live} />
        </div>
      ))
    ),
  );
  return (
    <section
      className="steps tc"
      aria-label="Activity"
      onKeyDown={(event) => trailKeys(event, () => setCopied(true))}
      onFocus={(event) => {
        const target = event.target as HTMLElement;
        setCopied(false);
        setFocus(keyboardOnRow(target) ? { copy: target.querySelector("[data-command]") !== null } : null);
      }}
      onBlur={() => {
        setFocus(null);
        setCopied(false);
      }}
    >
      <SurfacedWorkflows value={several}>{rows}</SurfacedWorkflows>
      {several ? <WorkflowCards items={group.items} threadId={threadId} live={live} /> : null}
      {focus ? (
        <div className="tchint">
          <span aria-hidden><kbd>↑</kbd><kbd>↓</kbd>Move</span>
          <span aria-hidden><kbd>→</kbd>Open</span>
          <span aria-hidden><kbd>←</kbd>Close</span>
          {focus.copy ? <span aria-hidden><kbd>C</kbd>Copy command</span> : null}
          {copied ? <span role="status">Copied</span> : null}
        </div>
      ) : null}
    </section>
  );
}

/** The running turn's header, with a rule beneath it. */
function Working({ row, tools }: { row: WorkingRow; tools: ToolItem[] }) {
  const now = useNow(true);
  return (
    <div className="work">
      <p className="work-summary" role="status">
        Working for {elapsed(row.since, now)}
      </p>
      <hr />
      {tools.length > 1 ? <RunningStack tools={tools} /> : null}
    </div>
  );
}

/**
 * The summary of a finished turn. What it hides mounts when first opened and then stays, so
 * closing animates; find reads the log rather than this tree, and opens the fold when it needs it.
 */
function Fold({ fold, live }: { fold: TurnFold; live: boolean }) {
  const threadId = useThreadOfTimeline();
  const [open, setOpenNow] = useOpenState(`fold:${fold.key}`, false);
  const [loaded, setLoaded] = useOpenState(`fold-loaded:${fold.key}`, false);
  const setOpen = (value: boolean) => {
    setOpenNow(value);
    if (value) setLoaded(true);
  };
  return (
    <div className="work">
      <SurfacedWorkflows value>
      <Collapsible
        open={open}
        onOpenChange={setOpen}
        summaryClassName="work-summary"
        summary={
          <>
            <span>{fold.label}</span>
            <ChevronRight size={14} className="chev" aria-hidden />
          </>
        }
      >
        <div className="work-fold">
          {(open || loaded) && fold.entries.map((entry) => (
            <TimelineRow key={entry.key} entry={entry} live={false} tools={NO_TOOLS} />
          ))}
          {open || loaded ? <GroupControls entries={fold.entries} /> : null}
        </div>
      </Collapsible>
      </SurfacedWorkflows>
      {/* A workflow outlives the turn that launched it, so it stays in view. */}
      <WorkflowCards items={foldTools(fold.entries)} threadId={threadId} live={live} />
      <hr />
    </div>
  );
}

const NO_TOOLS: ToolItem[] = [];

/** The open-state ids of a turn's tool groups: a group's own, or the row's when it stands alone. */
function groupIds(entries: readonly TimelineEntry[]): { ids: string[]; groups: number } {
  let ids: string[] = [];
  let groups = 0;
  for (const entry of entries) {
    if (entry.kind === "fold") {
      const inner = groupIds(entry.entries);
      ids = ids.concat(inner.ids);
      groups += inner.groups;
    } else if (entry.kind === "work") {
      for (const segment of workSegments(entry.items)) {
        if (!("runs" in segment)) continue;
        for (const run of segment.runs) {
          groups += 1;
          ids.push(run.items.length === 1 ? `tool:${run.key}` : `tg:${run.key}`);
        }
      }
    }
  }
  return { ids, groups };
}

/**
 * A finished turn's calls by the outcome the engine reported. The count line belongs to every
 * turn with tool calls; the bulk controls join it only once there are four or more groups.
 */
function GroupControls({ entries }: { entries: readonly TimelineEntry[] }) {
  const { setMany } = useOpenControl();
  const threadId = useThreadOfTimeline();
  const { ids, groups } = useMemo(() => groupIds(entries), [entries]);
  const tools = useMemo(() => foldTools(entries).filter((item): item is ToolItem => item.kind === "tool"), [entries]);
  const runId = tools[0]?.runId ?? null;
  // The fold's own run says whether its calls are still running; the page's live flag also covers
  // a later run, so it must not decide this. A run with no recorded span has ended.
  const run = useThreadLogs((state) => (threadId && runId ? (state.logs[threadId]?.runs[runId] ?? null) : null));
  const live = run !== null && run.endedAt === null;
  const count = useMemo(() => turnOutcomeText(tools, live, run?.outcome === "cancelled"), [tools, live, run?.outcome]);
  if (tools.length === 0) return null;
  return (
    <div className="tcmini">
      <span className="cnt">{count}</span>
      {groups >= 4 ? (
        <>
          <button type="button" className="lk" onClick={() => setMany(ids, true)}>
            Expand all
          </button>
          <span className="sep" aria-hidden />
          <button type="button" className="lk" onClick={() => setMany(ids, false)}>
            Collapse all
          </button>
        </>
      ) : null}
    </div>
  );
}

function foldTools(entries: readonly TimelineEntry[]): WorkItem[] {
  return entries.flatMap((entry) =>
    entry.kind === "work" ? entry.items : entry.kind === "fold" ? foldTools(entry.entries) : [],
  );
}

export function TimelineRow({ entry, live, tools }: { entry: TimelineEntry; live: boolean; tools: ToolItem[] }) {
  switch (entry.kind) {
    case "work":
      return <WorkGroupView group={entry} live={live} />;
    case "fold":
      return <Fold fold={entry} live={live} />;
    case "working":
      return <Working row={entry} tools={tools} />;
    default:
      return <ThreadItemView item={entry} live={live} />;
  }
}

function ItemView({ item, live }: { item: Exclude<TimelineEntry, WorkGroup | TurnFold | WorkingRow>; live: boolean }) {
  switch (item.kind) {
    case "message":
      // What a message's Markdown and plan card remember is keyed by the message, so two messages
      // with the same <details> summary, or a revised plan, never share a state.
      return (
        <OpenStatePrefix value={item.key}>
          <Message item={item} live={live} />
        </OpenStatePrefix>
      );
    case "approval":
      return <ApprovalEntry item={item} />;
    case "plan":
      return <PlanRow item={item} live={live} />;
    case "diff":
      return <DiffSummary item={item} />;
    case "notice":
      return <Notice item={item} />;
    case "activity":
      return <Activity item={item} />;
    case "stopped":
      return <Stopped item={item} />;
  }
}

export const ThreadItemView = memo(ItemView);
