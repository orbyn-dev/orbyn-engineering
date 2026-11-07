import { PreparationRetryButton } from "./ThreadBanners";
import { useWorkspace } from "../../state/workspace";
import { MessageActions } from "./MessageActions";
import {
  Brain,
  ChevronRight,
  FileDiff as FileDiffIcon,
  OctagonAlert,
} from "lucide-react";
import { memo, useState } from "react";
import { elapsed } from "../../lib/format";
import { useNow } from "../../lib/useNow";
import { EngineGlyph } from "../../ui/badges";
import { Collapsible } from "../../ui/Collapsible";
import { OpenStatePrefix, useOpenState } from "../../ui/openState";
import { useUi } from "../../state/ui";
import { Button, Pill } from "../../ui/controls";
import { ApprovalCard, useResolvedApproval } from "./ApprovalCard";
import { SurfacedWorkflows, WorkflowCards } from "./WorkflowControls";
import { Markdown } from "./Markdown";
import { PlanCard } from "./PlanCard";
import { proposedPlanIn, textAroundPlan } from "./proposedPlan";
import { SentFiles } from "./SentFiles";
import {
  type TimelineEntry,
  type TurnFold,
  type WorkGroup,
  type WorkingRow,
  type WorkItem,
} from "./timeline";
import { callRuns, type CallRun } from "./toolCalls";
import { PlanRow, TrailRun, trailKeys } from "./ToolTrail";
import { TimelineThread, useThreadOfTimeline } from "./timelineThread";
import type {
  ActivityItem,
  ApprovalItem,
  DiffSummaryItem,
  MessageItem,
  NoticeItem,
  ReasoningItem,
  ToolItem,
} from "./projection";

export { TimelineThread };

function Message({ item, live }: { item: MessageItem; live: boolean }) {
  if (item.role === "user") {
    return (
      <div className="msg-user" data-minimap={item.key}>
        <span className="sr-only">You said:</span>
        <p className="msg-bubble">{item.text}</p>
        <SentFiles attachments={item.attachments} text={item.text} />
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

function Notice({ item }: { item: NoticeItem }) {
  const threadId = useThreadOfTimeline();
  const preparation = item.code === "workspace_preparation_failed" || item.code === "workspace_preparation_unavailable";
  if (item.tone === "neutral") {
    return <p className="notice">{item.text}</p>;
  }
  return (
    <div className="notice-error" role="alert" data-minimap={item.key}>
      <OctagonAlert size={16} aria-hidden />
      <div>
        <strong>Run failed</strong>
        <p>{item.text}</p>
        {item.code && !preparation ? <Pill>{item.code}</Pill> : null}
        {threadId && item.runId && item.code === "workspace_preparation_failed"
          ? <PreparationRetryButton threadId={threadId} runId={item.runId} /> : null}
      </div>
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

/** Tool calls read as rows by kind; reasoning sits between them where it happened. */
export function WorkGroupView({ group, live }: { group: WorkGroup; live: boolean }) {
  const threadId = useThreadOfTimeline();
  const [focused, setFocused] = useState(false);
  const segments: ({ reasoning: ReasoningItem } | { runs: CallRun[] })[] = [];
  for (const item of group.items) {
    if (item.kind === "reasoning") {
      segments.push({ reasoning: item });
      continue;
    }
    const last = segments.at(-1);
    if (last && "runs" in last) {
      const tools = last.runs.flatMap((run) => run.items);
      segments[segments.length - 1] = { runs: callRuns([...tools, item]) };
    } else {
      segments.push({ runs: callRuns([item]) });
    }
  }
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
      onKeyDown={trailKeys}
      onFocus={(event) => setFocused((event.target as HTMLElement).matches("button.tcr"))}
      onBlur={() => setFocused(false)}
    >
      {several ? <SurfacedWorkflows value>{rows}</SurfacedWorkflows> : rows}
      {several ? <WorkflowCards items={group.items} threadId={threadId} live={live} /> : null}
      {focused ? (
        <div className="tchint" aria-hidden>
          <span><kbd>↑</kbd><kbd>↓</kbd>Move</span>
          <span><kbd>→</kbd>Open</span>
          <span><kbd>←</kbd>Close</span>
          <span><kbd>C</kbd>Copy command</span>
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

function foldTools(entries: TimelineEntry[]): WorkItem[] {
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
  }
}

export const ThreadItemView = memo(ItemView);
