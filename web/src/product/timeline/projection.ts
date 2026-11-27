import { readMcpAppMarker } from "./mcpAppBridge";
import type { DiffSummaryItem, PlanStep, RunSpan, SentAttachment, SubagentActivity, ThreadItem, ThreadLog, TimelineEntry, WorkflowState } from "./types";
import type {
  ActionKind,
  AgUiEvent,
  Approval,
  ApprovalState,
  JsonValue,
  WireThreadEvent,
} from "../../api/types";

const WORKFLOW_STATES: readonly string[] = ["running", "completed", "failed", "stopped"];
export const emptyLog: ThreadLog = {
  events: [],
  seen: new Set(),
  lastSeq: 0,
  items: [],
  timeline: [],
  runs: {},
  subagents: [],
  usage: null,
  sent: {},
};

function record(value: JsonValue | undefined): { [key in string]?: JsonValue } | null {
  if (value === undefined) return null;
  return typeof value === "object" && value !== null && !Array.isArray(value) ? value : null;
}

/** The optional Snap Shot source on a sent attachment (docs/snapshots.md), read defensively. */
function isWireSource(value: JsonValue | undefined): boolean {
  const source = record(value);
  return source?.kind === "snap_shot"
    && typeof source.captured_at === "string"
    && typeof source.app_name === "string"
    && typeof source.window_title === "string";
}

const ACTION_KINDS: readonly string[] = [
  "command",
  "file_change",
  "tool",
  "computer",
  "browser",
  "network",
] satisfies ActionKind[];
const APPROVAL_STATES: readonly string[] = [
  "pending",
  "allowed",
  "denied",
  "expired",
] satisfies ApprovalState[];

/** Reads an approval from an activity snapshot; null when it isn't one. */
export function approvalFromContent(
  content: JsonValue,
  threadId: string,
  runId: string | null,
  ts: string,
): Approval | null {
  const body = record(content);
  if (!body) return null;
  const id = body.id ?? body.approval_id;
  if (typeof id !== "string") return null;
  const str = (key: string) => {
    const value = body[key];
    return typeof value === "string" ? value : null;
  };
  const kind = str("kind");
  const state = str("state");
  return {
    id,
    thread_id: str("thread_id") ?? threadId,
    run_id: str("run_id") ?? runId ?? "",
    kind: kind && ACTION_KINDS.includes(kind) ? (kind as ActionKind) : "tool",
    summary: str("summary") ?? "Approval requested",
    detail: body.detail ?? null,
    state: state && APPROVAL_STATES.includes(state) ? (state as ApprovalState) : "pending",
    decided_by: str("decided_by"),
    decided_at: str("decided_at"),
    created_at: str("created_at") ?? ts,
    via: viaFrom(body.via),
  };
}

/** The Code Mode origin of an approval, read loosely: it is only drawn, never decided on. */
function viaFrom(value: unknown): Approval["via"] {
  if (typeof value !== "object" || value === null) return null;
  const { code_run: run, call, script_sha256: sha } = value as Record<string, unknown>;
  return typeof run === "string" && typeof call === "number" && typeof sha === "string"
    ? { code_run: run, call, script_sha256: sha }
    : null;
}

function planSteps(content: JsonValue): PlanStep[] {
  const body = record(content);
  const raw = Array.isArray(content)
    ? content
    : (body?.steps ?? body?.entries ?? body?.items ?? null);
  if (!Array.isArray(raw)) return [];
  const steps: PlanStep[] = [];
  for (const entry of raw) {
    if (typeof entry === "string") {
      steps.push({ text: entry, status: null });
      continue;
    }
    const step = record(entry);
    if (!step) continue;
    const text = step.title ?? step.text ?? step.content ?? step.description;
    if (typeof text !== "string") continue;
    steps.push({ text, status: typeof step.status === "string" ? step.status : null });
  }
  return steps;
}

function diffFiles(content: JsonValue): DiffSummaryItem["files"] {
  const body = record(content);
  const raw = Array.isArray(content) ? content : (body?.files ?? null);
  if (!Array.isArray(raw)) return [];
  const files: DiffSummaryItem["files"] = [];
  for (const entry of raw) {
    const file = record(entry);
    if (!file || typeof file.path !== "string") continue;
    files.push({
      path: file.path,
      additions: typeof file.additions === "number" ? file.additions : null,
      deletions: typeof file.deletions === "number" ? file.deletions : null,
    });
  }
  return files;
}

function outcomeType(outcome: JsonValue | null): string | null {
  const body = outcome === null ? null : record(outcome);
  return typeof body?.type === "string" ? body.type : null;
}

/** Mutable working copy used while folding one batch. */
interface Fold {
  threadId: string;
  items: ThreadItem[];
  index: Map<string, number>;
  timeline: TimelineEntry[];
  runs: Record<string, RunSpan>;
  subagents: SubagentActivity[];
  usage: JsonValue | null;
  /** What each run's message carried, by run id. */
  sent: Record<string, readonly SentAttachment[]>;
}

/** A note that is the same every time, or one worded from the event and the thread's engine. */
type LifecycleNote = string | ((value: JsonValue, engineName: string | null) => string);

/** The engine's display name, or the neutral stand-in when the catalogue has not named it. */
const engineLabel = (engineName: string | null): string => engineName?.trim() || "The engine";

/** Things that happened to the conversation rather than in it. */
const LIFECYCLE_NOTES: Record<string, LifecycleNote> = {
  "engine.session_restarted": "Agent session restarted.",
  "engine.session_lost": "The engine lost this conversation, so it continues in a new session.",
  "engine.switched": "Continued on another engine.",
  "context.compacted": "Context compacted.",
  "thread.rewound": (value, engineName) => {
    const engine = engineLabel(engineName);
    // Only a native rollback may say the engine went back: every other rewind hands over text.
    return record(value)?.native === true
      ? `Went back to before this message. ${engine} continues in its own session from there.`
      : `Continued from before this message in a new session. ${engine} was given the earlier conversation as text, not its own history.`;
  },
  "steer.failed": "The engine refused a message mid-turn, so it moved to the queue.",
  "permission.updated": "Permission mode changed; it applies from the next turn.",
};

function lifecycleNote(name: string, value: JsonValue, engineName: string | null): string | null {
  const note = LIFECYCLE_NOTES[name];
  if (note === undefined) return null;
  return typeof note === "string" ? note : note(value, engineName);
}

function put(fold: Fold, item: ThreadItem): void {
  const at = fold.index.get(item.key);
  if (at === undefined) {
    fold.index.set(item.key, fold.items.length);
    fold.items.push(item);
  } else {
    fold.items[at] = item;
  }
}

function patch<K extends ThreadItem["kind"]>(
  fold: Fold,
  key: string,
  kind: K,
  update: (item: Extract<ThreadItem, { kind: K }>) => Extract<ThreadItem, { kind: K }>,
): void {
  const at = fold.index.get(key);
  if (at === undefined) return;
  const item = fold.items[at];
  if (item?.kind === kind) {
    fold.items[at] = update(item as Extract<ThreadItem, { kind: K }>);
  }
}

function applyEvent(fold: Fold, wire: WireThreadEvent, engineName: string | null): void {
  const event: AgUiEvent = wire.event;
  const base = { seq: wire.seq, ts: wire.ts, runId: wire.run_id };
  // Engines reuse ids between runs; each run keeps its own rows.
  const scoped = (kind: string, id: string) => `${kind}:${wire.run_id ?? ""}:${id}`;
  const log = (label: string, detail: string | null, tone: TimelineEntry["tone"] = "neutral") =>
    fold.timeline.push({ seq: wire.seq, ts: wire.ts, label, detail, tone });
  const endRun = (outcome: RunSpan["outcome"]) => {
    const id = wire.run_id;
    if (!id) return;
    const span = fold.runs[id];
    fold.runs[id] = { startedAt: span?.startedAt ?? wire.ts, endedAt: wire.ts, outcome };
  };

  switch (event.type) {
    case "RUN_STARTED":
      if (wire.run_id) fold.runs[wire.run_id] = { startedAt: wire.ts, endedAt: null, outcome: null };
      log("Run started", null);
      break;
    case "RUN_FINISHED": {
      const outcome = outcomeType(event.outcome);
      endRun(outcome === "cancelled" ? "cancelled" : "success");
      if (outcome === "cancelled") {
        put(fold, {
          ...base,
          key: `notice:${wire.seq}`,
          kind: "notice",
          tone: "neutral",
          text: "Stopped",
          code: "run_stopped",
        });
      }
      log(outcome === "cancelled" ? "Run cancelled" : "Run finished", null);
      break;
    }
    case "RUN_ERROR":
      endRun("failed");
      put(fold, {
        ...base,
        key: `notice:${wire.seq}`,
        kind: "notice",
        tone: "error",
        text: event.message,
        code: event.code,
      });
      log("Run failed", event.message, "error");
      break;
    case "TEXT_MESSAGE_START": {
      const carried = event.role === "user" && wire.run_id ? fold.sent[wire.run_id] : undefined;
      put(fold, {
        ...base,
        key: scoped("msg", event.messageId),
        kind: "message",
        role: event.role,
        text: "",
        done: false,
        ...(carried && carried.length > 0 ? { attachments: carried } : {}),
      });
      break;
    }
    case "TEXT_MESSAGE_CONTENT":
      patch(fold, scoped("msg", event.messageId), "message", (item) => ({
        ...item,
        text: item.text + event.delta,
      }));
      break;
    case "TEXT_MESSAGE_END":
      patch(fold, scoped("msg", event.messageId), "message", (item) => ({ ...item, done: true }));
      break;
    case "REASONING_MESSAGE_START":
      put(fold, {
        ...base,
        key: scoped("rsn", event.messageId),
        kind: "reasoning",
        text: "",
        done: false,
      });
      break;
    case "REASONING_MESSAGE_CONTENT":
      patch(fold, scoped("rsn", event.messageId), "reasoning", (item) => ({
        ...item,
        text: item.text + event.delta,
      }));
      break;
    case "REASONING_MESSAGE_END":
      patch(fold, scoped("rsn", event.messageId), "reasoning", (item) => ({ ...item, done: true }));
      break;
    case "TOOL_CALL_START":
      put(fold, {
        ...base,
        key: scoped("tool", event.toolCallId),
        kind: "tool",
        callId: event.toolCallId,
        name: event.toolCallName,
        args: "",
        output: "",
        result: null,
        done: false,
        exitCode: null,
      });
      log(`Tool: ${event.toolCallName}`, null);
      break;
    case "TOOL_CALL_ARGS":
      patch(fold, scoped("tool", event.toolCallId), "tool", (item) => ({
        ...item,
        args: item.args + event.delta,
      }));
      break;
    case "TOOL_CALL_END":
      patch(fold, scoped("tool", event.toolCallId), "tool", (item) => ({ ...item, done: true, endedAt: item.endedAt ?? wire.ts }));
      break;
    case "TOOL_CALL_RESULT":
      patch(fold, scoped("tool", event.toolCallId), "tool", (item) => ({
        ...item,
        result: event.content,
        done: true,
        endedAt: item.endedAt ?? wire.ts,
      }));
      break;
    case "ACTIVITY_SNAPSHOT": {
      const key = scoped("act", event.messageId);
      // AG-UI: `replace: false` keeps an activity that already exists.
      if (event.replace === false && fold.index.has(key)) break;
      const common = { ...base, key };
      if (event.activityType === "approval") {
        const approval = approvalFromContent(event.content, fold.threadId, wire.run_id, wire.ts);
        if (approval) {
          put(fold, { ...common, kind: "approval", approval });
          log(
            approval.state === "pending" ? "Approval requested" : `Approval ${approval.state}`,
            approval.summary,
            approval.state === "pending" ? "attention" : "neutral",
          );
          break;
        }
      }
      if (event.activityType === "plan") {
        put(fold, { ...common, kind: "plan", steps: planSteps(event.content) });
        log("Plan updated", null);
        break;
      }
      if (event.activityType === "diff") {
        const files = diffFiles(event.content);
        put(fold, { ...common, kind: "diff", files });
        log("Files changed", files.length === 1 ? "1 file" : `${files.length} files`);
        break;
      }
      const message = record(event.content)?.message;
      if (event.activityType === "unmanaged" && typeof message === "string") {
        put(fold, { ...common, kind: "notice", tone: "neutral", text: message, code: null });
        log("Unmanaged run", null, "attention");
        break;
      }
      put(fold, {
        ...common,
        kind: "activity",
        activityType: event.activityType,
        content: event.content,
      });
      log(event.activityType, null);
      break;
    }
    case "CUSTOM": {
      const note = lifecycleNote(event.name, event.value, engineName);
      if (note !== null) {
        put(fold, { ...base, key: `notice:${wire.seq}`, kind: "notice", tone: "neutral", text: note, code: null });
        log(note, null);
      } else if (event.name === "user.attachments") {
        const body = record(event.value);
        const entries = Array.isArray(body?.attachments) ? body.attachments : [];
        const sent = entries.flatMap((entry): SentAttachment[] => {
          const file = record(entry);
          return typeof file?.id === "string" &&
            typeof file.name === "string" &&
            typeof file.mime === "string" &&
            typeof file.size === "number"
            ? [{
              id: file.id,
              name: file.name,
              mime: file.mime,
              size: file.size,
              ...(isWireSource(file.source) ? { source: file.source as SentAttachment["source"] } : {}),
            }]
            : [];
        });
        if (wire.run_id) fold.sent[wire.run_id] = sent;
        if (sent.length > 0) {
          const at = fold.items.findIndex(
            (item) => item.kind === "message" && item.role === "user" && item.runId === wire.run_id,
          );
          if (at >= 0) {
            const item = fold.items[at];
            if (item?.kind === "message") fold.items[at] = { ...item, attachments: sent };
          }
        }
      } else if (event.name === "engine.retrying") {
        const message = record(event.value)?.message;
        log("Engine retrying", typeof message === "string" ? message : null, "attention");
      } else if (event.name === "tool.output") {
        const body = record(event.value);
        const delta = body?.delta;
        if (typeof body?.toolCallId === "string" && typeof delta === "string") {
          patch(fold, scoped("tool", body.toolCallId), "tool", (item) => ({
            ...item,
            output: item.output + delta,
          }));
        }
      } else if (event.name === "tool.outcome") {
        const body = record(event.value);
        const outcome = body?.outcome;
        if (typeof body?.toolCallId === "string" && (outcome === "success" || outcome === "failure")) {
          patch(fold, scoped("tool", body.toolCallId), "tool", (item) => ({ ...item, outcome }));
        }
      } else if (event.name === "mcp_app.context") {
        const body = record(event.value);
        const server = typeof body?.server === "string" ? body.server : "An app";
        const text = `Sent ${server}'s app note with this message`;
        put(fold, { ...base, key: `notice:${wire.seq}`, kind: "notice", tone: "neutral", text, code: null });
        log(text, null);
      } else if (event.name === "mcp_app") {
        const marker = readMcpAppMarker(event.value);
        if (marker) {
          const { toolCallId, ...app } = marker;
          patch(fold, scoped("tool", toolCallId), "tool", (item) => ({ ...item, app }));
        }
      } else if (event.name === "workflow.state") {
        const body = record(event.value);
        const state = body?.state;
        if (typeof body?.toolCallId === "string" && typeof state === "string" && WORKFLOW_STATES.includes(state)) {
          const name = typeof body.name === "string" ? body.name : null;
          patch(fold, scoped("tool", body.toolCallId), "tool", (item) => ({
            ...item,
            workflow: { state: state as WorkflowState, name: name ?? item.workflow?.name ?? null },
          }));
        }
      } else if (event.name === "workflow.stop_failed") {
        const body = record(event.value);
        const message = body?.message;
        if (typeof body?.toolCallId === "string" && typeof message === "string") {
          patch(fold, scoped("tool", body.toolCallId), "tool", (item) =>
            item.workflow ? { ...item, workflow: { ...item.workflow, stopError: message } } : item,
          );
        }
      } else if (event.name === "tool.finished") {
        const body = record(event.value);
        const code = body?.exitCode;
        if (typeof body?.toolCallId === "string" && typeof code === "number") {
          patch(fold, scoped("tool", body.toolCallId), "tool", (item) => ({ ...item, exitCode: code }));
        }
      } else if (event.name === "subagent.activity") {
        const body = record(event.value);
        const text = (key: string) => {
          const value = body?.[key];
          return typeof value === "string" ? value : null;
        };
        const id = text("id");
        if (id) {
          const next = { id, tool: text("tool"), status: text("status"), agentPath: text("agentPath") };
          fold.subagents = [...fold.subagents.filter((agent) => agent.id !== id), next];
        }
      } else if (event.name === "usage") {
        fold.usage = event.value;
      } else if (event.name === "background.stopped") {
        // A task the engine stopped when the run ended; one row per task, under its run.
        const body = record(event.value);
        const taskId = body?.taskId;
        const taskType = typeof body?.taskType === "string" ? body.taskType : "";
        const described = typeof body?.description === "string" && body.description !== "" ? body.description : null;
        // A workflow keeps its own state (workflow item); it never gets a stopped row.
        if (typeof taskId === "string" && taskId !== "" && taskType !== "local_workflow") {
          const key = "stopped:" + (wire.run_id ?? "");
          const at = fold.items.findIndex((item) => item.key === key);
          const task = { taskId, taskType, description: described };
          if (at === -1) {
            put(fold, { ...base, key, kind: "stopped", tasks: [task] });
          } else {
            const item = fold.items[at];
            if (item?.kind === "stopped") fold.items[at] = { ...item, tasks: [...item.tasks, task] };
          }
        }
      }
      break;
    }
    case "RAW":
      break;
    default: {
      // An event type added on the server must not break an older client.
      const _unknown: never = event;
      void _unknown;
    }
  }
}

/** The finite `through` a rewind names, or null when the payload names none. */
function rewoundThrough(wire: WireThreadEvent): number | null {
  if (wire.event.type !== "CUSTOM" || wire.event.name !== "thread.rewound") return null;
  const through = record(wire.event.value)?.through;
  return typeof through === "number" && Number.isFinite(through) ? through : null;
}

/**
 * The events a rewind removed: after its `through` and before the rewind itself.
 * The log keeps every event; only the projection leaves these out, so the result
 * is the same however the batches were split or ordered.
 */
function withoutRewound(events: readonly WireThreadEvent[]): readonly WireThreadEvent[] {
  const ranges: { after: number; before: number }[] = [];
  for (const wire of events) {
    const through = rewoundThrough(wire);
    if (through !== null && through < wire.seq) ranges.push({ after: through, before: wire.seq });
  }
  if (ranges.length === 0) return events;
  return events.filter((wire) => !ranges.some(({ after, before }) => wire.seq > after && wire.seq < before));
}

function fold(
  threadId: string,
  log: ThreadLog,
  events: readonly WireThreadEvent[],
  incremental: boolean,
  engineName: string | null,
): Pick<ThreadLog, "items" | "timeline" | "runs" | "subagents" | "usage" | "sent"> {
  const working: Fold = incremental
    ? {
        threadId,
        items: [...log.items],
        index: new Map(log.items.map((item, at) => [item.key, at])),
        timeline: [...log.timeline],
        runs: { ...log.runs },
        subagents: [...log.subagents],
        usage: log.usage,
        sent: { ...log.sent },
      }
    : {
        threadId,
        items: [],
        index: new Map(),
        timeline: [],
        runs: {},
        subagents: [],
        usage: null,
        sent: {},
      };
  for (const event of withoutRewound(events)) applyEvent(working, event, engineName);
  const { items, timeline, runs, subagents, usage, sent } = working;
  return { items, timeline, runs, subagents, usage, sent };
}

/**
 * Folds a batch into the log. Replayed seqs are dropped; a seq that arrives
 * behind the log's head is inserted in order and the view is rebuilt. A rewind
 * rebuilds too, since it hides items an earlier batch already folded.
 * `engineName` is the thread's engine display name, for a note that names it.
 */
export function applyEvents(
  threadId: string,
  log: ThreadLog,
  incoming: readonly WireThreadEvent[],
  engineName: string | null = null,
): ThreadLog {
  const seen = new Set(log.seen);
  const fresh: WireThreadEvent[] = [];
  for (const event of incoming) {
    if (seen.has(event.seq)) continue;
    seen.add(event.seq);
    fresh.push(event);
  }
  if (fresh.length === 0) return log;
  fresh.sort((a, b) => a.seq - b.seq);

  const first = fresh[0];
  const inOrder = first !== undefined && first.seq > log.lastSeq;
  const rewound = fresh.some((event) => rewoundThrough(event) !== null);
  const events = inOrder
    ? [...log.events, ...fresh]
    : [...log.events, ...fresh].sort((a, b) => a.seq - b.seq);
  const projected = inOrder && !rewound
    ? fold(threadId, log, fresh, true, engineName)
    : fold(threadId, emptyLog, events, false, engineName);
  return {
    events,
    seen,
    lastSeq: Math.max(log.lastSeq, fresh.at(-1)?.seq ?? 0),
    ...projected,
  };
}
