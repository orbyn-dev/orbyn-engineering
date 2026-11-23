import type { McpAppView } from "./mcpAppBridge";
import type { Approval, AttachmentSource, JsonValue, WireThreadEvent } from "../../api/types";

export interface ItemBase {
  key: string;
  seq: number;
  ts: string;
  runId: string | null;
}

/** A file the message carried, as the server recorded it at send time. */
export interface SentAttachment {
  id: string;
  name: string;
  mime: string;
  size: number;
  /** Where a Snap Shot came from, when the service kept it (docs/snapshots.md). */
  source?: AttachmentSource;
}

export interface MessageItem extends ItemBase {
  kind: "message";
  role: string;
  text: string;
  done: boolean;
  /** Attachments this user message carried; absent when it had none. */
  attachments?: readonly SentAttachment[];
  /** Where this mid-turn message went, when it was sent into a running turn. */
  steer?: "pending" | "delivered" | "queued" | "failed";
}
export interface ReasoningItem extends ItemBase {
  kind: "reasoning";
  text: string;
  done: boolean;
}
export interface ToolItem extends ItemBase {
  kind: "tool";
  /** The engine's id for the call, which the server's item record also uses. */
  callId: string;
  name: string;
  args: string;
  /** Output streamed while the tool runs, until its final result arrives. */
  output: string;
  result: string | null;
  done: boolean;
  /** When the call's end or result arrived; the row's duration runs from `ts` to here. */
  endedAt?: string;
  /** What a command exited with, when the engine reports it. */
  exitCode: number | null;
  /** The driver's reported protocol outcome; absent in older logs. */
  outcome?: "success" | "failure";
  /** The engine workflow this call launched, while the engine reports one. */
  workflow?: ToolWorkflow;
  /** The MCP App page the service stored for this call. */
  app?: McpAppView;
}
export type WorkflowState = "running" | "completed" | "failed" | "stopped";
export interface ToolWorkflow {
  state: WorkflowState;
  name: string | null;
  /** The engine refused the last stop. */
  stopError?: string;
}
export interface ApprovalItem extends ItemBase {
  kind: "approval";
  approval: Approval;
}
export interface PlanStep {
  text: string;
  status: string | null;
}
export interface PlanItem extends ItemBase {
  kind: "plan";
  steps: PlanStep[];
}
export interface DiffSummaryItem extends ItemBase {
  kind: "diff";
  files: { path: string; additions: number | null; deletions: number | null }[];
}
export interface ActivityItem extends ItemBase {
  kind: "activity";
  activityType: string;
  content: JsonValue;
}
export interface NoticeItem extends ItemBase {
  kind: "notice";
  tone: "error" | "neutral";
  text: string;
  code: string | null;
}
/** One task the engine stopped when its run ended, from a background.stopped event. */
export interface StoppedTask {
  taskId: string;
  taskType: string;
  /** The engine's own words for the task; null when it said nothing. */
  description: string | null;
}
export interface StoppedItem extends ItemBase {
  kind: "stopped";
  /** One row per task, in the order the engine reported them. */
  tasks: StoppedTask[];
}

export type ThreadItem =
  | MessageItem
  | ReasoningItem
  | ToolItem
  | ApprovalItem
  | PlanItem
  | DiffSummaryItem
  | ActivityItem
  | NoticeItem
  | StoppedItem;

export interface TimelineEntry {
  seq: number;
  ts: string;
  label: string;
  detail: string | null;
  tone: "neutral" | "error" | "attention";
}

/** One turn's span and how it ended, read from its own events. */
export interface RunSpan {
  startedAt: string;
  endedAt: string | null;
  outcome: "success" | "cancelled" | "failed" | null;
}

/** A delegated agent the engine reported on this thread's stream. */
export interface SubagentActivity {
  id: string;
  tool: string | null;
  status: string | null;
  agentPath: string | null;
}

export interface ThreadLog {
  /** Every accepted event, ascending by seq. */
  events: readonly WireThreadEvent[];
  seen: ReadonlySet<number>;
  lastSeq: number;
  items: readonly ThreadItem[];
  timeline: readonly TimelineEntry[];
  runs: Readonly<Record<string, RunSpan>>;
  subagents: readonly SubagentActivity[];
  /** The latest usage report, as the engine sent it. */
  usage: JsonValue | null;
  /** What each run's message carried, kept until that run's message starts. */
  sent: Readonly<Record<string, readonly SentAttachment[]>>;
}
