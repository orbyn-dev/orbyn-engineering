/** mock-fixture: deterministic data for the dev server and tests. Never shipped. */
import type {
  Approval,
  EngineStatus,
  EngineTools,
  ToolEntry,
  ToolState,
  EngineUsage,
  FileDiff,
  Project,
  Run,
  RunState,
  ScheduledTask,
  ServerInfo,
  Thread,
  WireThreadEvent,
  AgUiEvent,
  JsonValue,
  ServerSettings,
  TaskRun,
  UpdateStatus,
  WebhookDelivery,
  WorkspaceStatus,
  LimitsOverview,
} from "../src/api/types.ts";

/** Fixed under test. In the dev server, "now" at startup, so ages read realistically. */
export const T0 = process.env.VITEST ? Date.UTC(2026, 9, 31, 9, 0, 0) : Date.now();
export const iso = (offsetSeconds: number) => new Date(T0 + offsetSeconds * 1000).toISOString();

export const info: ServerInfo = { version: "0.1.0", remote: false, platform: "macos", environment_label: "Test computer" };

export const projects: Project[] = [
  {
    id: "p-app",
    name: "orbyn-engineering-app",
    path: "/Users/dev/code/orbyn-engineering-app",
    trusted: true,
    is_git: true,
    managed: false,
    scratch: false,
    created_at: iso(-86400 * 3),
  },
  {
    id: "p-site",
    name: "marketing-site",
    path: "/Users/dev/code/marketing-site",
    trusted: true,
    is_git: true,
    managed: false,
    scratch: false,
    created_at: iso(-86400 * 2),
  },
  {
    id: "p-new",
    name: "vendor-sdk-with-a-rather-long-repository-name-for-layout",
    path: "/Users/dev/code/clients/acme/vendor-sdk-with-a-rather-long-repository-name-for-layout",
    trusted: false,
    is_git: false,
    managed: false,
    scratch: false,
    created_at: iso(-3600),
  },
];

const caps = (over: Partial<EngineStatus["capabilities"]> = {}): EngineStatus["capabilities"] => ({
  streaming: true,
  goal_control: false, decline_feedback: false,
  permission_modes: ["supervised", "auto_edits", "auto", "full_access"],
  approvals: true,
  resume: true,
  mcp: true,
  sandbox: true,
  managed: true,
  skill_delivery: ["native_plugin", "mcp_skill_server"],
  ...over,
});

export const engines: EngineStatus[] = [
  {
    id: "claude",
    name: "Claude Code",
    tier: "native",
    capabilities: caps(),
    installed: true,
    path: "/Users/dev/.local/bin/claude",
    version: "2.1.14",
    auth: "logged_in",
    installer: "npm",
    latest: "2.1.20",
    support: "tested",
    update_command: "npm install -g @anthropic-ai/claude-code",
    manual_hint: null,
    sign_in_command: "claude auth login",
    updating: false,
    problem: null,
    account: "sam@example.com",
  },
  {
    id: "codex",
    name: "Codex",
    tier: "native",
    capabilities: caps({ skill_delivery: ["native_skill_root", "mcp_skill_server"] }),
    installed: true,
    path: "/opt/homebrew/bin/codex",
    version: "0.58.0",
    auth: "logged_out",
    installer: "homebrew",
    latest: "0.58.0",
    support: "untested",
    update_command: null,
    manual_hint: "Installed outside a package manager Orbyn can prove. Update it the way you installed it.",
    sign_in_command: "codex login",
    updating: false,
    problem: null,
  },
  {
    id: "gemini-acp",
    name: "Gemini CLI",
    tier: "acp",
    capabilities: caps({ resume: false, sandbox: false, skill_delivery: ["prompt_bootstrap"] }),
    installed: true,
    path: "/usr/local/bin/gemini",
    version: "0.9.2",
    auth: "unknown",
    installer: "unknown",
    latest: null,
    support: "below_minimum",
    update_command: null,
    manual_hint: "Update Gemini CLI with the tool you used to install it.",
    sign_in_command: null,
    updating: false,
    problem: "Version 0.9.2 is below the minimum Orbyn supports, 0.10.0.",
  },
  {
    id: "openbot",
    name: "OpenBot connector",
    tier: "agui",
    capabilities: caps({ mcp: false, sandbox: false, resume: false, skill_delivery: [] }),
    installed: true,
    path: "https://openbot.internal.example/agui",
    version: null,
    auth: "not_applicable",
    installer: "unknown",
    latest: null,
    support: "unknown",
    update_command: null,
    manual_hint: null,
    sign_in_command: null,
    updating: false,
    problem: null,
  },
  {
    id: "aider",
    name: "Aider (terminal)",
    tier: "terminal",
    capabilities: caps({
      streaming: false,
      approvals: false,
      resume: false,
      mcp: false,
      sandbox: false,
      managed: false,
      skill_delivery: [],
    }),
    installed: true,
    path: "/Users/dev/.local/bin/aider",
    version: "0.86.1",
    auth: "not_applicable",
    installer: "unknown",
    latest: null,
    support: "unknown",
    update_command: null,
    manual_hint: "Aider is a custom terminal engine. Update it yourself.",
    sign_in_command: null,
    updating: false,
    problem: null,
  },
  {
    id: "cursor",
    name: "Cursor",
    tier: "acp",
    capabilities: caps({ skill_delivery: [] }),
    installed: true,
    path: "/usr/local/bin/cursor-agent",
    version: null,
    auth: "not_applicable",
    installer: "unknown",
    latest: null,
    support: "unknown",
    update_command: null,
    manual_hint: null,
    sign_in_command: null,
    updating: false,
    problem: null,
  },
  {
    id: "kimi",
    name: "Kimi Code",
    tier: "acp",
    capabilities: caps({ skill_delivery: [] }),
    installed: true,
    path: "/usr/local/bin/kimi",
    version: null,
    auth: "not_applicable",
    installer: "unknown",
    latest: null,
    support: "unknown",
    update_command: null,
    manual_hint: null,
    sign_in_command: null,
    updating: false,
    problem: null,
  },
  {
    id: "cursor-agent",
    name: "Cursor Agent",
    tier: "acp",
    capabilities: caps({ skill_delivery: [] }),
    installed: false,
    path: null,
    version: null,
    auth: "unknown",
    installer: "unknown",
    latest: null,
    support: "unknown",
    update_command: null,
    manual_hint: null,
    sign_in_command: null,
    updating: false,
    problem: "Not found on PATH.",
  },
];

const run = (id: string, threadId: string, state: RunState, offset: number, error?: string): Run => ({
  id,
  thread_id: threadId,
  state,
  error: error ?? null,
  started_at: iso(offset),
  ended_at: state === "completed" || state === "failed" || state === "cancelled" || state === "interrupted" ? iso(offset + 90) : null,
});

const baseThread = (
  id: string,
  projectId: string,
  title: string,
  engine: string,
  offset: number,
  over: Partial<Thread> = {},
): Thread => ({
  id,
  project_id: projectId,
  title,
  engine,
  model: null,
  cwd: projects.find((p) => p.id === projectId)?.path ?? "/",
  worktree: null,
  skill_profile: null,
  active_run: null,
  last_run_state: null,
  scratch: false,
  created_at: iso(offset - 600),
  updated_at: iso(offset),
  lifecycle: lifecycle(),
  ...over,
});

const manyTitles = [
  "Tidy CI matrix",
  "Bump Rust toolchain",
  "Explain run fencing",
  "Trace audit writes",
  "Fix clippy warnings",
  "Port path handling to Windows",
  "Review migration order",
  "Write store tests",
  "Add policy glob tests",
  "Drop unused feature flags",
  "Update release notes",
  "Check WAL checkpointing",
  "Refactor engine probe",
  "Trim startup logs",
  "Rename skill profile field",
  "Add terminal resize test",
  "Document tray behaviour",
  "Investigate flaky SSE test",
  "Cut 0.1.0 candidate",
  "Remove dead fixtures",
  "Harden origin checks",
  "Sort audit by id",
  "Measure SQLite writes",
  "Clean worktree on delete",
];

export function threads(): Thread[] {
  const list: Thread[] = [
    baseThread("t-approval", "p-app", "Add approval timeout handling to the run owner", "claude", -300, {
      model: "claude-opus-5-5",
      active_run: run("r-approval", "t-approval", "awaiting_approval", -360),
      last_run_state: "awaiting_approval",
      worktree: {
        path: "/Users/dev/code/orbyn-engineering-app-wt/approval-timeout",
        branch: "approval-timeout",
        base: "main",
      },
    }),
    baseThread("t-done", "p-app", "Explain how the event log resumes", "claude", -1800, {
      last_run_state: "completed",
      model: "claude-opus-5-5",
    }),
    baseThread("t-tools", "p-app", "Tool call states", "claude", -1700, {
      last_run_state: "completed",
      model: "claude-opus-5-5",
    }),
    baseThread("t-tools-live", "p-app", "Tool calls, running", "claude", -120, {
      active_run: run("r-tools-live", "t-tools-live", "running", -130),
      last_run_state: "running",
      model: "claude-opus-5-5",
    }),
    baseThread("t-failed", "p-app", "Upgrade tokio and fix the fallout", "codex", -3600, {
      last_run_state: "failed",
    }),
    baseThread("t-interrupted", "p-app", "Backfill audit rows after crash", "claude", -5400, {
      last_run_state: "interrupted",
    }),
    baseThread("t-unmanaged", "p-site", "Rewrite hero copy with aider", "aider", -7200, {
      last_run_state: "completed",
    }),
    baseThread("t-site-approval", "p-site", "Push a preview of the pricing page", "claude", -600, {
      active_run: run("r-site", "t-site-approval", "awaiting_approval", -640),
      last_run_state: "awaiting_approval",
    }),
    baseThread("t-empty", "p-site", "New thread", "claude", -9000),
  ];
  manyTitles.forEach((title, at) => {
    list.push(
      baseThread(`t-many-${at}`, "p-app", title, at % 3 === 0 ? "codex" : "claude", -10000 - at * 3600, {
        last_run_state: at % 7 === 3 ? "failed" : at % 11 === 5 ? "cancelled" : "completed",
      }),
    );
  });
  return list;
}

/** A task's last firings, newest first, one of each outcome the service reports. */
export function taskRuns(taskId: string): TaskRun[] {
  const run = (at: number, minutes: number | null, outcome: string, extra: Partial<TaskRun> = {}): TaskRun => ({
    id: `${taskId}-run-${-at}`, task_id: taskId, source: "schedule", started_at: iso(at * 86400),
    ended_at: minutes === null ? null : iso(at * 86400 + minutes * 60), outcome, thread_id: minutes === null ? null : `${taskId}-thread-${-at}`, error: null, ...extra,
  });
  return [
    run(1, null, "queued"),
    run(0, 6, "running", { ended_at: null }),
    run(-1, 9, "completed"),
    run(-2, 4, "failed", { error: "The agent stopped." }),
    run(-3, 2, "cancelled"),
    run(-4, 11, "interrupted"),
    run(-5, null, "refused", { error: "webhook answer 401" }),
    run(-6, null, "skipped", { error: "limit window 92% used" }),
  ];
}

/** What reached a webhook: a known sender, one the service only knows by its User-Agent, and one with none. */
export function webhookDeliveries(taskId: string): WebhookDelivery[] {
  const one = (id: string, status: string, at: number, extra: Partial<WebhookDelivery> = {}): WebhookDelivery => ({
    id, task_id: taskId, status, received_at: iso(at), headers: {}, body: "{}", prompt: null, thread_id: null, ...extra,
  });
  return [
    one("dlv-1", "accepted", -600, { from: "GitHub", thread_id: "t-done" }),
    one("dlv-2", "filtered", -3600, { from: "GitLab" }),
    one("dlv-3", "bad_signature", -90000, { from: `Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36 Edg/124.0` }),
    one("dlv-4", "duplicate", -90600),
  ];
}

/** How many conversations each engine keeps in its own history folder. */
export const nativeSessionCounts: Record<string, number> = { claude: 12, codex: 6 };

export function scheduledTasks(): ScheduledTask[] {
  return [
    {
      id: "task-review",
      project_id: "p-app",
      thread_id: "t-approval",
      name: "Daily review",
      prompt: "Review the open changes.",
      engine: "claude",
      schedule: { kind: "interval", every_minutes: 30, time: null, weekdays: [], utc_offset_minutes: 0, only_on_success: false },
      enabled: true,
      signature: null,
      has_secret: false,
      filters: [],
      dedupe: null,
      max_age_seconds: null,
      timestamp_path: null,
      next_run_at: iso(600),
      last_run_at: iso(-1200),
      last_status: "started",
      created_at: iso(-86400),
    },
    {
      id: "task-hook",
      project_id: "p-app",
      thread_id: "t-approval",
      name: "Incoming hook",
      prompt: "Handle the webhook.",
      engine: "claude",
      schedule: { kind: "webhook", every_minutes: null, time: null, weekdays: [], utc_offset_minutes: 0, only_on_success: false },
      enabled: false,
      signature: null,
      has_secret: false,
      filters: [],
      dedupe: null,
      max_age_seconds: null,
      timestamp_path: null,
      next_run_at: null,
      last_run_at: null,
      last_status: "never",
      created_at: iso(-86400),
    },
    {
      id: "task-elsewhere",
      project_id: "p-app",
      thread_id: "t-done",
      name: "Explain the event log",
      prompt: "Explain how the event log resumes.",
      engine: "claude",
      schedule: { kind: "fixed_time", every_minutes: null, time: "09:00", weekdays: [1, 2, 3, 4, 5], utc_offset_minutes: 0, only_on_success: false },
      enabled: true,
      signature: null,
      has_secret: false,
      filters: [],
      dedupe: null,
      max_age_seconds: null,
      timestamp_path: null,
      next_run_at: null,
      last_run_at: null,
      last_status: "never",
      created_at: iso(-86400),
    },
  ];
}

export function pendingApprovals(): Approval[] {
  return [
    {
      id: "ap-1",
      thread_id: "t-approval",
      run_id: "r-approval",
      kind: "command",
      summary: "Run the core test suite",
      detail: { command: "cargo test -p orbyn-core approval", cwd: "/Users/dev/code/orbyn-engineering-app-wt/approval-timeout" },
      state: "pending",
      decided_by: null,
      decided_at: null,
      created_at: iso(-320),
    },
    {
      id: "ap-2",
      thread_id: "t-approval",
      run_id: "r-approval",
      kind: "file_change",
      summary: "Edit 2 files in crates/orbyn-core",
      detail: { paths: ["crates/orbyn-core/src/approvals.rs", "crates/orbyn-core/src/run_owner.rs"] },
      state: "pending",
      decided_by: null,
      decided_at: null,
      created_at: iso(-300),
    },
    {
      id: "ap-3",
      thread_id: "t-site-approval",
      run_id: "r-site",
      kind: "network",
      summary: "Publish a preview deployment",
      detail: { url: "https://preview.example.com/deploy", method: "POST" },
      state: "pending",
      decided_by: null,
      decided_at: null,
      created_at: iso(-620),
    },
  ];
}

let seqCounter = 0;
const logOf = (threadId: string, runId: string, offset: number, events: AgUiEvent[]) =>
  events.map((event, at): WireThreadEvent => ({
    seq: 0,
    thread_id: threadId,
    run_id: runId,
    ts: iso(offset + at * 2),
    event: event,
  }));

export function sequence(events: WireThreadEvent[]): WireThreadEvent[] {
  seqCounter = 0;
  return events.map((event) => ({ ...event, seq: ++seqCounter }));
}

const user = (id: string, text: string): AgUiEvent[] => [
  { type: "TEXT_MESSAGE_START", messageId: id, role: "user" },
  { type: "TEXT_MESSAGE_CONTENT", messageId: id, delta: text },
  { type: "TEXT_MESSAGE_END", messageId: id },
];

const assistant = (id: string, text: string): AgUiEvent[] => [
  { type: "TEXT_MESSAGE_START", messageId: id, role: "assistant" },
  { type: "TEXT_MESSAGE_CONTENT", messageId: id, delta: text },
  { type: "TEXT_MESSAGE_END", messageId: id },
];

const reasoning = (id: string, text: string): AgUiEvent[] => [
  { type: "REASONING_MESSAGE_START", messageId: id, role: "assistant" },
  { type: "REASONING_MESSAGE_CONTENT", messageId: id, delta: text },
  { type: "REASONING_MESSAGE_END", messageId: id },
];

const tool = (id: string, name: string, args: string, result: string): AgUiEvent[] => [
  { type: "TOOL_CALL_START", toolCallId: id, toolCallName: name },
  { type: "TOOL_CALL_ARGS", toolCallId: id, delta: args },
  { type: "TOOL_CALL_END", toolCallId: id },
  { type: "TOOL_CALL_RESULT", messageId: `${id}-r`, toolCallId: id, content: result, role: "tool" },
];

/** The exit code the driver reports once a command ends. */
const exited = (id: string, exitCode: number): AgUiEvent => ({ type: "CUSTOM", name: "tool.finished", value: { toolCallId: id, exitCode } });

const hundredLines = Array.from({ length: 214 }, (_, at) => `  ✓ e2e/case-${String(at + 1).padStart(2, "0")}.spec.ts ${40 + (at % 9) * 7}ms`).join("\n");

const approvalSnapshot = (approval: Approval): AgUiEvent => ({
  type: "ACTIVITY_SNAPSHOT",
  messageId: `approval-${approval.id}`,
  activityType: "approval",
  content: approval as unknown as JsonValue,
  replace: true,
});

export const approvalEvent = approvalSnapshot;

const longAnswer = `The log is the source of truth. Every run event is stored in \`thread_events\` first, then broadcast, so delivery to a client is only a projection.

## How a client resumes

1. The client keeps the highest \`seq\` it has applied.
2. On reconnect it sends that value as \`Last-Event-ID\`.
3. The service replays everything after it, then follows live.

Nothing is lost, because a live subscriber is added **before** the replay query runs. Duplicates are possible at that seam, which is why the client drops a \`seq\` it has already seen.

\`\`\`rust
let mut live = bus.subscribe(thread_id);
let replay = store.events_after(thread_id, last_seq)?;
for event in replay.into_iter().chain(live.dedupe_after(last_seq)) {
    sink.send(event).await?;
}
\`\`\`

| Case | Result |
| --- | --- |
| Tab sleeps for an hour | Full replay from the last seq |
| Service restarts mid-run | Run becomes \`interrupted\`, stream replays to the end |
| Same event twice | Second copy is dropped by \`seq\` |

> A snapshot is not an event barrier. Only \`seq\` orders the log.

See [the AG-UI event spec](https://docs.ag-ui.com/concepts/events) for the event names. A very long path to check wrapping: \`crates/orbyn-core/src/runs/owner/fencing/compare_and_set_previous_state_transition.rs\`.
`;

const longToolOutput = Array.from(
  { length: 60 },
  (_, at) => `test approvals::tests::case_${String(at).padStart(3, "0")}_expires_open_approvals_when_the_driver_exits ... ok`,
).join("\n");

export function logs(): Record<string, WireThreadEvent[]> {
  const pending = pendingApprovals();
  const [ap1, ap2, ap3] = pending as [Approval, Approval, Approval];
  return {
    "t-approval": sequence(
      logOf("t-approval", "r-approval", -360, [
        { type: "RUN_STARTED", threadId: "t-approval", runId: "r-approval" },
        ...user("u1", "When the driver exits with an approval still open, the engine hangs. Expire it and send a denial."),
        ...reasoning("rs1", "The approval lifecycle says driver exit expires open approvals and the engine receives a denial. I should find where the driver exit is observed, then check whether the compare-and-set covers a late decision."),
        ...tool("c1", "Read", '{"path":"crates/orbyn-core/src/run_owner.rs"}', "pub struct RunOwner {\n    run_id: RunId,\n    epoch: OwnerEpoch,\n    ...\n}"),
        ...tool("c2", "Grep", '{"pattern":"expire_open","path":"crates/orbyn-core/src"}', "crates/orbyn-core/src/approvals.rs:88: pub fn expire_open(&self, run: RunId)"),
        {
          type: "ACTIVITY_SNAPSHOT",
          messageId: "plan-1",
          activityType: "plan",
          content: { steps: [
            { title: "Find where driver exit is observed", status: "completed" },
            { title: "Expire open approvals on exit", status: "in_progress" },
            { title: "Send the denial to the engine", status: "pending" },
            { title: "Add a regression test", status: "pending" },
          ] },
          replace: true,
        },
        ...assistant("a1", "`expire_open` exists but is only called on cancel. I'll call it from the exit path and add a test. I need to run the tests and edit two files."),
        {
          type: "ACTIVITY_SNAPSHOT",
          messageId: "diff-1",
          activityType: "diff",
          content: { files: [
            { path: "crates/orbyn-core/src/approvals.rs", additions: 14, deletions: 2 },
            { path: "crates/orbyn-core/src/run_owner.rs", additions: 6, deletions: 1 },
          ] },
          replace: true,
        },
        approvalSnapshot(ap1),
        approvalSnapshot(ap2),
      ]),
    ),
    "t-site-approval": sequence(
      logOf("t-site-approval", "r-site", -640, [
        { type: "RUN_STARTED", threadId: "t-site-approval", runId: "r-site" },
        ...user("u1", "Deploy a preview of the pricing page."),
        ...assistant("a1", "The build passed. Publishing the preview needs your approval."),
        approvalSnapshot(ap3),
      ]),
    ),
    "t-done": sequence([
      ...logOf("t-done", "r-done", -1900, [
        { type: "RUN_STARTED", threadId: "t-done", runId: "r-done" },
        ...user("u1", "Explain how the event log resumes after a reconnect. Include the edge cases."),
        ...reasoning("rs1", "I should read the architecture document first, then the store query."),
        ...tool("c1", "Read", '{"path":"docs/architecture.md"}', "## Event stream\n\nEvery run event is stored in `thread_events` with a per-thread `seq`, then broadcast."),
        ...tool("c2", "Bash", '{"command":"cargo test -p orbyn-core events"}', longToolOutput),
        ...assistant("a1", longAnswer),
        { type: "RUN_FINISHED", threadId: "t-done", runId: "r-done", outcome: { type: "success" } },
      ]),
    ]),
    "t-tools": sequence([
      ...logOf("t-tools", "r-tools", -1800, [
        { type: "RUN_STARTED", threadId: "t-tools", runId: "r-tools" },
        ...user("u1", "Run the tests and check the redirect still works."),
        ...tool("c1", "Bash", '{"command":"git status --short"}', " M auth/redirect.ts\n M auth/redirect.test.ts\n?? auth/next.ts"),
        exited("c1", 0),
        ...tool("c2", "Bash", JSON.stringify({ command: 'rg -n "next" auth/redirect.ts auth/session.ts' }), "auth/redirect.ts:13:  const next = req.query.next"),
        exited("c2", 0),
        ...tool("c3", "Bash", '{"command":"npm test -- --reporter=dot"}', " ✓ auth/redirect.test.ts (6)\n ✗ e2e/login.spec.ts › returns to the page after sign-in\n   AssertionError: expected \"/home\" to equal \"/settings\"\n\n Tests  1 failed | 47 passed (48)"),
        exited("c3", 1),
        ...tool("c4", "Read", '{"file_path":"/Users/dev/code/orbyn-engineering-app/auth/redirect.ts","offset":12,"limit":7}', "    12→export function after(req) {\n    13→  const next = req.query.next\n    14→  if (!next) return \"/\"\n    15→  const url = new URL(next, req.origin)\n    16→  if (url.host !== req.host) return \"/\"\n    17→  return url.pathname + url.search\n    18→}"),
        ...tool("c5", "Read", '{"file_path":"/Users/dev/code/orbyn-engineering-app/auth/session.ts"}', "     1→export const session = {}"),
        ...tool("c6", "Edit", JSON.stringify({ file_path: "/Users/dev/code/orbyn-engineering-app/auth/redirect.ts", old_string: '  return "/home"', new_string: '  const next = req.query.next\n  return inside(next) ? next : "/"' }), "The file has been updated."),
        ...tool("c7", "Write", JSON.stringify({ file_path: "/Users/dev/code/orbyn-engineering-app/auth/next.ts", content: 'export function inside(next) {\n  if (!next) return false\n  return !next.startsWith("//")\n}' }), "File created successfully at: /Users/dev/code/orbyn-engineering-app/auth/next.ts"),
        ...tool("c8", "Grep", '{"pattern":"AGENTS.md"}', "AGENTS.md:1:# Orbyn Engineering library instructions\nCLAUDE.md:3:@AGENTS.md\ndocs/agents/roles.md:14:Read AGENTS.md before you edit"),
        ...tool("c9", "LS", '{"path":"/Users/dev/code/orbyn-engineering-app/auth"}', "- /Users/dev/code/orbyn-engineering-app/auth/\n  - redirect.ts\n  - redirect.test.ts\n  - session.ts"),
        ...tool("c10", "WebFetch", '{"url":"https://cheatsheetseries.owasp.org/cheatsheets/Unvalidated_Redirects_and_Forwards_Cheat_Sheet.html"}', "Redirects and forwards are common. Check the destination against an allowlist."),
        ...tool("c11", "mcp__orbyn-memory__find", '{"query":"login redirect","scope":"project","limit":5}', "Redirects must stay on the app's own host"),
        ...tool("c12", "Bash", '{"command":"python3 -m unittest discover -s tests -v"}', hundredLines),
        exited("c12", 0),
        ...tool("c13", "Bash", '{"command":"git diff --exit-code auth/"}', ""),
        exited("c13", 0),
        ...assistant("a1", "The redirect ignored `next`. I changed it and the tests now expect the original page."),
        { type: "RUN_FINISHED", threadId: "t-tools", runId: "r-tools", outcome: { type: "success" } },
      ]),
    ]),
    "t-tools-live": sequence([
      ...logOf("t-tools-live", "r-tools-live", -130, [
        { type: "RUN_STARTED", threadId: "t-tools-live", runId: "r-tools-live" },
        ...user("u1", "Run the tests."),
        ...tool("l1", "Bash", '{"command":"git status --short"}', " M auth/redirect.ts"),
        exited("l1", 0),
        { type: "TOOL_CALL_START", toolCallId: "l2", toolCallName: "Bash" },
        { type: "TOOL_CALL_ARGS", toolCallId: "l2", delta: '{"command":"python3 -m unittest discover -s tests -v"}' },
        { type: "CUSTOM", name: "tool.output", value: { toolCallId: "l2", delta: "  ✓ e2e/case-01.spec.ts 47ms\n  ✓ e2e/case-02.spec.ts 54ms\n  … e2e/case-03.spec.ts" } },
      ]),
    ]),
    "t-failed": sequence(
      logOf("t-failed", "r-failed", -3700, [
        { type: "RUN_STARTED", threadId: "t-failed", runId: "r-failed" },
        ...user("u1", "Upgrade tokio to the latest release and fix whatever breaks."),
        ...tool("c1", "Bash", '{"command":"cargo update -p tokio"}', "Updating tokio v1.40.0 -> v1.48.0"),
        { type: "TEXT_MESSAGE_START", messageId: "a1", role: "assistant" },
        { type: "TEXT_MESSAGE_CONTENT", messageId: "a1", delta: "The update applied. Three call sites use a removed" },
        { type: "RUN_ERROR", message: "Codex exited unexpectedly with status 137.", code: "engine_exit" },
      ]),
    ),
    "t-interrupted": sequence(
      logOf("t-interrupted", "r-int", -5500, [
        { type: "RUN_STARTED", threadId: "t-interrupted", runId: "r-int" },
        ...user("u1", "Backfill the missing audit rows."),
        ...assistant("a1", "Audit rows are append-only, so a backfill has to be new rows that name the gap."),
      ]),
    ),
    "t-unmanaged": sequence(
      logOf("t-unmanaged", "r-un", -7300, [
        { type: "RUN_STARTED", threadId: "t-unmanaged", runId: "r-un" },
        ...user("u1", "Rewrite the hero copy to be shorter."),
        ...assistant("a1", "Done in the terminal session. See the Terminal panel for the output."),
        { type: "RUN_FINISHED", threadId: "t-unmanaged", runId: "r-un", outcome: { type: "success" } },
      ]),
    ),
  };
}

export const replyEvents = (runId: string, threadId: string, prompt: string, messageId: string): AgUiEvent[] => [
  { type: "RUN_STARTED", threadId, runId },
  ...user(`u-${messageId}`, prompt),
  ...reasoning(`r-${messageId}`, "This mock run replays a fixed script so the interface can be checked without an engine."),
  ...tool(`c-${messageId}`, "Read", '{"path":"README.md"}', "# orbyn-engineering-app"),
];

export const replyText =
  "This is a **scripted reply** from the mock service. It streams in small pieces so you can check `reconnect`, ordering and cancel.\n\n- The text arrives word by word\n- Each word has its own `seq`\n\nStop the run with Cancel, or wait for it to finish.";

export const diffs: Record<string, FileDiff[]> = {
  "t-approval": [
    {
      path: "crates/orbyn-core/src/approvals.rs",
      status: "modified",
      additions: 14,
      deletions: 2,
      patch: `@@ -84,8 +84,20 @@ impl Approvals {
     /// Expires every open approval for a run and denies the engine.
-    pub fn expire_open(&self, run: RunId) {
-        self.store.expire(run);
+    pub fn expire_open(&self, run: RunId) -> Result<usize> {
+        let expired = self.store.expire_pending(run)?;
+        for approval in &expired {
+            self.notify_denied(approval)?;
+        }
+        Ok(expired.len())
     }
+
+    fn notify_denied(&self, approval: &Approval) -> Result<()> {
+        self.waiters
+            .take(&approval.id)
+            .map_or(Ok(()), |waiter| waiter.send(Decision::Deny))
+    }
 
     pub fn decide(&self, id: &ApprovalId, allow: bool) -> Result<Outcome> {
         self.store.compare_and_set(id, Pending, allow.into())
`,
    },
    {
      path: "crates/orbyn-core/src/run_owner.rs",
      status: "modified",
      additions: 6,
      deletions: 1,
      patch: `@@ -212,7 +212,12 @@ impl RunOwner {
     async fn on_driver_exit(&mut self, status: ExitStatus) -> Result<()> {
-        self.finish(status.into()).await
+        // Open approvals must not outlive the process that asked.
+        self.approvals.expire_open(self.run_id)?;
+        self.finish(status.into()).await
     }
`,
    },
    {
      path: "crates/orbyn-core/tests/approval_expiry_with_a_long_test_file_name_to_check_row_wrapping.rs",
      status: "added",
      additions: 22,
      deletions: 0,
      patch: `@@ -0,0 +1,22 @@
+use orbyn_core::testing::harness;
+
+#[tokio::test]
+async fn driver_exit_expires_open_approvals() {
+    let run = harness().await.start_run().await;
+    let approval = run.request_approval("cargo test").await;
+    run.kill_driver().await;
+    assert_eq!(approval.state().await, ApprovalState::Expired);
+}
`,
    },
  ],
  "t-done": [],
};

export const fsTree: Record<string, { parent: string | null; entries: { name: string; is_git?: boolean }[] }> = {
  "/Users/dev": {
    parent: "/Users",
    entries: [{ name: "code" }, { name: "Documents" }, { name: "Downloads" }],
  },
  "/Users": { parent: "/", entries: [{ name: "dev" }, { name: "Shared" }] },
  "/": { parent: null, entries: [{ name: "Users" }, { name: "opt" }] },
  "/Users/dev/code": {
    parent: "/Users/dev",
    entries: [
      { name: "orbyn-engineering-app", is_git: true },
      { name: "marketing-site", is_git: true },
      { name: "payments-service", is_git: true },
      { name: "scratch" },
    ],
  },
};

/** The frozen comparison state: six recent threads, one working, 43 settled. */
export function referenceState(now: number) {
  const ago = (seconds: number) => new Date(now - seconds * 1000).toISOString();
  const project = (id: string, name: string): Project => ({
    id,
    name,
    path: `/Users/dev/code/${name}`,
    trusted: true,
    is_git: true,
    managed: false,
    scratch: false,
    created_at: ago(864000),
  });
  const refProjects = [project("rp-1", "draftdesk"), project("rp-2", "orbitkit"), project("rp-3", "ledgerline")];
  const row = (
    id: string,
    projectId: string,
    title: string,
    branch: string,
    seconds: number,
    state: RunState = "completed",
  ): Thread => ({
    id,
    project_id: projectId,
    title,
    engine: "claude",
    model: "Opus 5.5",
    cwd: `/Users/dev/code/${projectId}`,
    worktree: { path: `/Users/dev/code/${projectId}-wt/${branch}`, branch, base: "main" },
    scratch: false,
    skill_profile: null,
    active_run: null,
    last_run_state: state,
    created_at: ago(seconds + 600),
    updated_at: ago(seconds),
  });
  const started = ago(25);
  const active: Thread = {
    ...row("rt-active", "rp-1", "Add Archive Export For Drafts", "main", 18, "running"),
    active_run: { id: "rr-active", thread_id: "rt-active", state: "running", error: null, started_at: started, ended_at: null },
  };
  const refThreads: Thread[] = [
    row("rt-1", "rp-1", "Offline drafts and sync status page do", "dev/offline-drafts-sync", 7 * 60),
    row("rt-2", "rp-1", "Require sign-in before exporting", "dev/require-sign-in-export", 29 * 60),
    row("rt-3", "rp-2", "Built-in Log Viewer Filters", "dev/log-viewer-filters", 3700),
    row("rt-4", "rp-3", "Reserve ledgerline on npm for pre-release", "main", 7300),
    row("rt-5", "rp-1", "Dark Theme Report Polish", "dev/dark-report", 4 * 3600),
    row("rt-6", "rp-3", "Disk and memory check", "dev/health", 90),
    active,
    ...Array.from({ length: 43 }, (_, at) =>
      row(`rt-old-${at}`, at % 2 ? "rp-2" : "rp-3", `Settled task ${at + 1}`, "main", 2 * 86400 + at * 3600),
    ),
  ];
  const tool = (id: string, name: string): AgUiEvent => ({ type: "TOOL_CALL_START", toolCallId: id, toolCallName: name });
  const at = (seconds: number, event: AgUiEvent, seq: number): WireThreadEvent => ({
    seq,
    thread_id: "rt-active",
    run_id: "rr-active",
    ts: ago(seconds),
    event,
  });
  const events: AgUiEvent[] = [
    { type: "RUN_STARTED", threadId: "rt-active", runId: "rr-active" },
    ...user("ru1", "Add archive export to drafts with 4 parallel tasks"),
    ...reasoning("rr1", "Four independent pieces can run at the same time."),
    tool("rc1", "Export button"),
    tool("rc2", "Archive API"),
    tool("rc3", "Archive tests"),
    tool("rc4", "Archive docs"),
    ...assistant("ra1", "Okay, starting the four export tasks."),
  ];
  const stamps = [45, 45, 45, 45, 39, 35, 30, 25, 23, 21, 19, 18, 18, 18];
  const log = events.map((event, index) => at(stamps[index] ?? 18, event, index + 1));
  return { projects: refProjects, threads: refThreads, logs: { "rt-active": log } as Record<string, WireThreadEvent[]> };
}

const rev = (seed: string) => `${seed}0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b`.slice(0, 40);

export const incoming = [
  { sha: rev("a1b2c3d"), subject: "Add statechart review guidance" },
  { sha: rev("d4e5f6a"), subject: "Tighten worktree cleanup rules" },
];

export function workspaceStatus(list: Project[], configured = true): WorkspaceStatus {
  return {
    configured,
    default_url: "https://example.com/orbyn/skills.git",
    library: configured
      ? {
          path: "/Users/dev/Library/Application Support/Orbyn/workspace",
          adopted: false,
          revision: rev("8ede3c4"),
          branch: "main",
          dirty: false,
          dirty_files: [],
          upstream: "origin/main",
          ahead: 0,
          behind: 2,
          previous_revision: rev("51fa223"),
          native_skills: true,
          problem: null,
        }
      : null,
    targets: list.map((project, at) => ({
      project_id: project.id,
      path: project.path,
      git_root: project.path,
      revision: rev(at === 0 ? "c16b80a" : "85a1821"),
      branch: "main",
      dirty: at === 1,
      worktrees:
        at === 0
          ? [
              {
                path: "/Users/dev/code/orbyn-engineering-app-wt/approval-timeout",
                branch: "approval-timeout",
                revision: rev("9c8d7e6"),
                dirty: false,
              },
            ]
          : [],
      instructions: ["AGENTS.md"],
      same_name_skills: at === 0 ? ["tdd"] : [],
      problem: null,
    })),
  };
}


export function updateStatus(behind: number): UpdateStatus {
  return {
    checked_at: new Date(T0 - 600_000).toISOString(),
    error: null,
    engines: [
      { id: "claude", current: "2.1.14", latest: "2.1.20", support: "tested", can_apply: true, blocked_reason: null },
      { id: "codex", current: "0.58.0", latest: "0.58.0", support: "untested", can_apply: false, blocked_reason: null },
    ],
    library: {
      current: rev("8ede3c4"),
      latest: rev("a1b2c3d"),
      behind,
      ahead: 0,
      dirty: false,
      incoming: behind > 0 ? incoming : [],
      can_apply: true,
      blocked_reason: null,
    },
    app: {
      current: "0.1.0",
      latest: "0.2.0",
      notes: "## Changes\n\n- Faster thread loading\n- Fixes terminal resize on Windows",
      mode: "desktop",
      can_apply: true,
      blocked_reason: null,
      unconfigured: false,
    },
  };
}

export const serverSettings = (): ServerSettings => ({
  auto_settle: { after_days: null, on_merge: false, keep_pull_requests: true },
  snooze_presets: [],
  title_instructions: null,
});

export const slashCommands = [
  { name: "review", description: "Review the current diff", source: "engine" },
  { name: "clear", description: "Start with a clean context", source: "engine" },
];

export const lifecycle = (): NonNullable<Thread["lifecycle"]> => ({
  pinned_at: null,
  archived_at: null,
  settled_at: null,
  snoozed_until: null,
  unread: false,
  auto_settle: null,
  position: 0,
  parent_thread_id: null,
  relation: null,
  permission_mode: "supervised",
  plan_mode: false,
});

export const usageEngines = (): EngineUsage[] => {
  const days = [
    { date: iso(-86_400).slice(0, 10), tokens: 1_250_000, estimated_cost: 1.25 },
    { date: iso(0).slice(0, 10), tokens: 3_000_000, estimated_cost: 3 },
  ];
  return [
    {
      engine: "claude",
      status: "scanned",
      files_scanned: 3,
      files_skipped: 0,
      models: [
        {
          model: "claude-sonnet",
          input_tokens: 120_000,
          output_tokens: 90_000,
          cache_read_tokens: 4_000_000,
          cache_write_tokens: 40_000,
          estimated_cost: 4.25,
          cost_split: { input: 0.36, output: 1.35, cache_read: 1.2, cache_write: 1.34 },
          tiers: [{ tier: "standard", tokens: 4_250_000, estimated_cost: 4.25 }],
          unpriced_tokens: 0,
          days,
        },
      ],
      days,
    },
  ];
};

const effort = {
  id: "effort",
  label: "Reasoning",
  values: [
    { id: "low", label: "Low", default: false },
    { id: "medium", label: "Medium", default: true },
    { id: "high", label: "High", default: false },
  ],
};

export const engineModels = (engine: string) => ({
  engine,
  source: "documented",
  error: null,
  fallback: false,
  checked_at: iso(0),
  models: [
    { id: "sonnet", label: "Claude Sonnet", description: null, is_default: true, custom: false, options: [effort] },
    { id: "opus", label: "Claude Opus", description: null, is_default: false, custom: false, options: [effort] },
    { id: "haiku", label: "Claude Haiku", description: null, is_default: false, custom: false, options: [] },
  ],
});

/** The Pull requests page's rows in every data state, with the details the service adds. */
const listed = (
  number: number,
  title: string,
  over: Record<string, unknown> = {},
  project = "p-app",
) => ({
  project_id: project,
  pull: {
    number,
    title,
    url: "https://github.com/orbyn-dev/orbyn-engineering/pull/" + number,
    state: "open",
    head: "feat/" + title.toLowerCase().replace(/[^a-z]+/g, "-").slice(0, 14),
    base: "main",
    author: "riley",
    draft: false,
    updated_at: iso(-240),
    labels: [],
    review_state: null,
    checks_state: "pass",
    checks: { total: 4, passing: 4, failing: 0, running: 0, failing_names: [] as string[] },
    ...over,
  },
});
/** One pull request page per state, for the screens that draw each: 201 running, 202 failed, 203 review asked of you, 204 changes requested, 205 approved, 206 conflict, 207 merged, 208 closed, 209 draft, 210 opened by the signed-in person. */
export const pageStates: Record<number, { pull: Record<string, unknown>; mergeable: string; reviewers: string[]; askedYou?: boolean }> = (() => {
  const base = (number: number, over: Record<string, unknown>, mergeable = "mergeable", reviewers = ["sam"], askedYou = false) => ({
    pull: {
      number,
      title: "Return to the page people came from",
      url: "https://github.com/orbyn-dev/orbyn-engineering/pull/" + number,
      state: "open",
      head: "fix/login",
      base: "main",
      author: "riley",
      draft: false,
      updated_at: iso(-240),
      labels: [],
      review_state: null,
      checks_state: "pass",
      checks: { total: 4, passing: 4, failing: 0, running: 0, failing_names: [] as string[] },
      ...over,
    },
    mergeable,
    reviewers,
    askedYou,
  });
  const running = { checks_state: "pending", checks: { total: 4, passing: 2, failing: 0, running: 2, failing_names: [] as string[] } };
  return {
    201: base(201, running),
    202: base(202, { checks_state: "fail", checks: { total: 4, passing: 3, failing: 1, running: 0, failing_names: ["e2e (macOS)"] } }),
    203: base(203, { review_state: "review_required" }, "mergeable", ["you"], true),
    // Changes asked of the signed-in person's own pull request: the one that needs them.
    204: base(204, { review_state: "changes_requested", author: "you" }),
    205: base(205, { review_state: "approved" }),
    206: base(206, { review_state: "approved" }, "conflicting"),
    207: base(207, { state: "merged", review_state: "approved" }),
    208: base(208, { state: "closed", review_state: "approved" }),
    209: base(209, { draft: true, ...running }, "mergeable", []),
    210: base(210, { author: "you", review_state: "review_required" }),
  };
})();
export const pageChecks = (number: number) => {
  const names = ["lint", "unit", "e2e (macOS)", "e2e (Linux)"];
  const pull = pageStates[number]?.pull as { checks?: { failing: number; running: number } } | undefined;
  return names.map((name, at) => ({
    name,
    state: pull?.checks?.failing && at === 2 ? "fail" : pull?.checks?.running && at >= 2 ? "pending" : "pass",
    url: "https://github.com/orbyn-dev/orbyn-engineering/actions/runs/" + (900 + at),
    workflow: "CI",
    description: pull?.checks?.failing && at === 2 ? "login ignores unsafe next values" : null,
  }));
};
export const pageFiles = () => [
  { path: "auth/redirect.ts", previous_path: null, status: "modified", additions: 2, deletions: 1, patch: "@@ -14,4 +14,4 @@\n export function after(req) {\n-  return \"/home\"\n+  return inside(next) ? next : \"/\"\n }", viewed: false },
  { path: "auth/redirect.test.ts", previous_path: null, status: "modified", additions: 2, deletions: 2, patch: "@@ -1,2 +1,2 @@\n-old\n+new", viewed: false },
];

export const pullList = () => [
  listed(142, "Return to the page people came from", {
    updated_at: iso(-240),
    review_state: "approved",
    checks_state: "fail",
    checks: { total: 4, passing: 3, failing: 1, running: 0, failing_names: ["e2e (macOS)"] },
  }),
  listed(139, "Project icon picker", {
    updated_at: iso(-7200),
    review_state: "changes_requested",
    checks_state: "fail",
    checks: { total: 5, passing: 2, failing: 3, running: 0, failing_names: ["e2e (macOS)", "e2e (Linux)", "docs build"] },
  }),
  listed(138, "Usage cost filters", {
    updated_at: iso(-18000),
    checks_state: "pending",
    checks: { total: 4, passing: 1, failing: 0, running: 2, failing_names: [] },
  }),
  listed(137, "Settings scoped rows", { updated_at: iso(-90000) }),
  listed(3, "Calendar feed token", {
    updated_at: iso(-100000),
    checks: { total: 4, passing: 2, failing: 0, running: 0, failing_names: [] },
  }, "p-site"),
  listed(131, "Rename workspace", {
    updated_at: iso(-180000),
    checks_state: "none",
    checks: { total: 2, passing: 0, failing: 0, running: 0, failing_names: [] },
  }),
  listed(136, "Flaky E2E shard", { updated_at: iso(-260000), checks_state: "none", checks: { total: 0, passing: 0, failing: 0, running: 0, failing_names: [] } }, "p-site"),
  listed(57, "Footer links", { updated_at: iso(-270000), draft: true, checks_state: "none", checks: undefined }),
];
export const pullDetails = () => [
  { project_id: "p-app", number: 142, created_at: iso(-86400), requested_reviewers: [{ name: "riley-chen", kind: "user" }, { name: "sam-reyes", kind: "user" }, { name: "marilyn", kind: "user" }], comment_count: 12, viewer_review_state: "approved" },
  { project_id: "p-app", number: 139, created_at: iso(-172800), requested_reviewers: [{ name: "riley-chen", kind: "user" }, { name: "sam-reyes", kind: "user" }, { name: "marilyn", kind: "user" }], comment_count: 3, viewer_review_state: "changes_requested" },
  { project_id: "p-app", number: 138, created_at: iso(-200000), requested_reviewers: [{ name: "riley-chen", kind: "user" }, { name: "sam-reyes", kind: "user" }, { name: "marilyn", kind: "user" }], comment_count: 1, viewer_review_state: "commented" },
  { project_id: "p-app", number: 137, created_at: iso(-300000), requested_reviewers: [{ name: "riley-chen", kind: "user" }, { name: "sam-reyes", kind: "user" }, { name: "marilyn", kind: "user" }], comment_count: 2, viewer_review_state: "dismissed" },
  { project_id: "p-site", number: 3, created_at: iso(-310000), requested_reviewers: [{ name: "orbyn-dev/web", kind: "team" }, { name: "sam-reyes", kind: "user" }, { name: "marilyn", kind: "user" }], comment_count: 5, viewer_review_state: "pending" },
  { project_id: "p-app", number: 131, created_at: iso(-400000), requested_reviewers: [{ name: "you", kind: "user" }, { name: "sam-reyes", kind: "user" }, { name: "marilyn", kind: "user" }], comment_count: 0, viewer_review_state: "none" },
  { project_id: "p-site", number: 136, created_at: iso(-500000), requested_reviewers: [{ name: "riley-chen", kind: "user" }, { name: "sam-reyes", kind: "user" }, { name: "marilyn", kind: "user" }], comment_count: 1, viewer_review_state: "none" },
  { project_id: "p-app", number: 203, requested_reviewers: [{ name: "you", kind: "user" }], comment_count: 3, viewer_review_state: "none" },
  { project_id: "p-app", number: 205, requested_reviewers: [], comment_count: 3, viewer_review_state: "approved" },
  { project_id: "p-app", number: 210, requested_reviewers: [{ name: "sam", kind: "user" }], comment_count: 3, viewer_review_state: "none" },
  // A forge that gives none of these: every cell shows an en dash.
  { project_id: "p-app", number: 57 },
];

export const pulls = () => [
  {
    project_id: "orbyn-engineering-app",
    pull: {
      number: 42,
      title: "Add approval timeouts",
      url: "https://example.com/pull/42",
      state: "open",
      head: "approval-timeout",
      base: "main",
      author: "dev",
      draft: false,
      updated_at: iso(-600),
      labels: ["bug"],
      review_state: "approved",
      checks_state: "passing",
    },
  },
];

/** Skills shown only by the development mock API. */
export const skills = [
  { name: "orbyn-workflow", category: "Workflow and planning", description: "Coordinate the stages of engineering work.", enabled: true },
  { name: "architect", category: "Engineering practice", description: "Shape interfaces and responsibilities.", enabled: true },
  { name: "tdd", category: "Engineering practice", description: "Plan and verify focused behavior tests.", enabled: true },
  { name: "orbyn-security-audit", category: "Artifact investigation and REA maintenance", description: "Review a specified design or source boundary.", enabled: true },
  { name: "stack-diagram", category: "Product design", description: "Illustrate components and their relationships.", enabled: true },
  { name: "reflect", category: "Workflow and planning", description: "Manage approved local engineering notes.", enabled: true },
  { name: "orbyn-ui-design", category: "Product design", description: "Shape product flows and interfaces.", enabled: true },
  { name: "social-preview", category: "Social content and visuals", description: "Prepare previews for an announcement.", enabled: true },
  { name: "orbyn-upstream-review", category: "Engineering practice", description: "Inspect changes in upstream references.", enabled: true },
];

/** What an engine reports as available, as the service sends it: Claude after a run, Codex when asked. */
export function engineTools(engine: string): EngineTools {
  const entry = (name: string, state: ToolState, extra: Partial<ToolEntry> = {}): ToolEntry => ({ name, state, ...extra });
  if (engine.startsWith("codex")) {
    return {
      engine, source: "check", checked_at: "2026-11-08T14:02:00Z", reported: true, incomplete: false,
      not_reported: ["builtin"],
      groups: [
        { id: "orbyn", count: 5, items: [] },
        { id: "mcp", count: 1, items: [entry("filesystem", "ready", { tools: 14, description: "Read files in the project" })] },
        { id: "connectors", count: 3, items: [
          entry("GitHub", "ready", { description: "Repositories, issues and pull requests", source: "Codex app" }),
          entry("Gmail", "needs_sign_in", { description: "Search and read mail", source: "Codex app", sign_in_url: "https://chatgpt.com/apps/gmail" }),
          entry("Google Calendar", "starting", { description: "Read your calendar", source: "Codex app" }),
        ] },
        { id: "plugins", count: 1, items: [entry("orbyn-skills", "ready", { description: "3 commands" })] },
        { id: "skills", count: 39, items: [] },
      ],
    };
  }
  return {
    engine, source: "check+run", checked_at: "2026-11-08T14:02:00Z", reported: true, incomplete: false, not_reported: [],
    groups: [
      { id: "builtin", count: 3, items: ["Bash", "Read", "Edit"].map((name) => entry(name, "ready")) },
      { id: "orbyn", count: 5, items: [] },
      { id: "mcp", count: 2, items: [
        entry("github", "ready", { tools: 26, description: "Repos, issues and pull requests" }),
        entry("postgres-dev", "failed", { description: "Read-only queries on the dev database", reason: "Couldn’t start. Command not found.", detail: "Command not found: pg-mcp" }),
      ] },
      { id: "connectors", count: 2, items: [
        entry("Vercel", "ready", { description: "Deployments and logs", source: "claude.ai connector" }),
        entry("Google Drive", "needs_sign_in", { description: "Search and read your Drive files", source: "claude.ai connector", sign_in_url: "https://claude.ai/settings/connectors" }),
      ] },
      { id: "plugins", count: 1, items: [entry("orbyn-skills", "ready", { description: "3 commands · 2 agents" })] },
      { id: "skills", count: 39, items: [] },
    ],
  };
}

/** What GET /api/limits answers on the Limits scenarios: only Codex gives a window (the driver's own Claude windows are never drawn). */
export function limits(scenario: string, checkedAt: string): LimitsOverview {
  const window = (id: string, label: string, minutes: number, used: number, resetsInSeconds: number) => ({
    id, label, used_percent: used, window_minutes: minutes, resets_at: iso(resetsInSeconds),
  });
  const claude = {
    engine: "claude", checked_at: checkedAt, limits: {}, plan: "Personal",
    windows: [window("five_hour", "5 hours", 300, 41, 4800)],
  };
  const codex = (...windows: ReturnType<typeof window>[]) => ({ engine: "codex", checked_at: checkedAt, limits: {}, plan: null, windows });
  const weekly = window("secondary", "7 days", 10080, 62, 3 * 86_400);
  switch (scenario) {
    case "limits-near":
      return { pooled: [], engines: [claude, codex(window("primary", "5 hours", 300, 93, 38 * 60), weekly)], unavailable: [] };
    case "limits-reached":
      return { pooled: [], engines: [claude, codex(window("primary", "5 hours", 300, 100, 62 * 60), { ...weekly, used_percent: 71 })], unavailable: [] };
    case "limits-signedout":
      return { pooled: [], engines: [claude], unavailable: [{ engine: "codex", reason: "Not signed in." }] };
    case "limits-error":
      return { pooled: [], engines: [], unavailable: [{ engine: "codex", reason: "The engine did not answer." }] };
    case "limits-claude":
      return { pooled: [], engines: [claude], unavailable: [] };
    case "limits-providers":
      return { pooled: [], engines: [], unavailable: [] };
    default:
      return { pooled: [], engines: [claude, codex(weekly)], unavailable: [] };
  }
}
