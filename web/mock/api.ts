import type { ProjectScopesQuery } from "../src/generated/ProjectScopesQuery.ts";
import type { ProjectScopeCatalog } from "../src/generated/ProjectScopeCatalog.ts";
import type { ScopedPreferencesResult } from "../src/generated/ScopedPreferencesResult.ts";
import type { ScopedPreferencesChange } from "../src/generated/ScopedPreferencesChange.ts";
import type { JsonValue } from "../src/api/types.ts";
import type { InstallOutcome } from "../src/generated/InstallOutcome.ts";
import type { KeybindingRule } from "../src/generated/KeybindingRule.ts";
import type { KeybindingsView } from "../src/generated/KeybindingsView.ts";
import type { ModelInfo } from "../src/generated/ModelInfo.ts";
import type { ProviderEntry } from "../src/generated/ProviderEntry.ts";
import type { TextGeneratorEngine } from "../src/generated/TextGeneratorEngine.ts";
import type { TextGenerators } from "../src/generated/TextGenerators.ts";
/**
 * createMockApi: an in-memory service with the architecture's HTTP contract.
 * The dev-server plugin and the tests both drive it through `handle`.
 */
import type {
  AgUiEvent,
  ApplyAllResult,
  Approval,
  EngineModels,
  CleanupPreview,
  Diagnostics,
  EngineStatus,
  Project,
  ProjectFileView,
  QueuedMessage,
  ScheduledTask,
  ServerInfo,
  ServerSettings,
  ShellStatus,
  Thread,
  UpdateStatus,
  UpdateThread,
  WireThreadEvent,
  WorkspaceStatus,
} from "../src/api/types.ts";
import { createAccountMock } from "./account.ts";
import { createBacktestMock } from "./backtest.ts";
import { createBackgroundMock } from "./background.ts";
import { createCapacityMock } from "./capacity.ts";
import { createCodeModeMock } from "./codemode.ts";
import { createCrewMock } from "./crew.ts";
import { createEscapesMock } from "./escapes.ts";
import { createKeybindingsMock } from "./keybindings.ts";
import { createLessonsMock } from "./lessons.ts";
import { createSkillProposalsMock } from "./skillProposals.ts";
import { boardMock } from "./board.ts";
import { flowMock } from "./flow.ts";
import { createReviewControlsMock } from "./reviewControls.ts";
import { mapMock } from "./map.ts";
import { needsYouMock } from "./needsyou.ts";
import { createExplorerMock } from "./explorer.ts";
import { createSecretsMock } from "./secrets.ts";
import { createFoldersMock } from "./folders.ts";
import { createQuestionsMock } from "./questions.ts";
import { createTerminalsMock } from "./terminals.ts";
import * as fixtures from "./fixtures.ts";
import { acpRegistry, historyScan } from "./history.ts";
import { customModels, instanceEngines, providerInstances } from "./instances.ts";
import { cleanupPreview, cleanupSummary, diagnostics } from "./maintenance.ts";
import { providerList } from "./providers.ts";
import { preferenceCatalog, projectPreferenceKeys } from "./preferenceCatalog.ts";

export interface MockResponse {
  status: number;
  body?: unknown;
}

export type Scenario =
  | "default"
  | "empty"
  | "first-run"
  | "no-skills"
  | "updates-unconfigured"
  | "remote"
  | "offline"
  | "reference"
  /** The service on another computer, for the rows only macOS shows: "platform-windows", "platform-linux". */
  | `platform-${"windows" | "linux"}`
  /** Two checkouts of one repository (the app and a worktree of it) beside the other projects, so Settings can scope to the project or to one checkout. */
  | "settings-checkouts"
  /** The sign-in gate is on, signed out; see `mock/account.ts` for the throwaway accounts. */
  | "account"
  /** Crew Preview on (`crew`, `crew-<state>`): see `mock/crew.ts` for the states. Autopilot approvals: "crew-one-vendor". Autopilot by area: "crew-areas", "crew-areas-tripped", "crew-areas-ceiling-off", "crew-areas-recovering", "crew-areas-criteria-met", "crew-areas-restored", "crew-areas-lowdata", "crew-areas-stale". Guard, classes and routing: "crew-guard", "crew-guard-unknown", "crew-guard-full", "crew-guard-empty", "crew-classes", "crew-classes-invalid", "crew-classes-note", "crew-route-blocked", "crew-route-invalid", "crew-route-none". */
  /** Settings › Crew › Worker provider: "crew-worker-provider-any" starts on Any provider, "crew-worker-provider-fail" refuses the write. Pinned-ref block: "crew-classes-pinned". */
  /** Settings › Crew › Daily digest: "crew-digest-unsaved" has the 17:00 default with no offset yet (it sends nothing), "crew-digest-save-fail" refuses the save. */
  /** Live review fixtures: "crew-review-consent", "crew-review-same", "crew-review-full", "crew-review-remove". Background crews: "crew-background" (the four the designs draw), "crew-background-empty". */
  /** Escape log, Crew Designs v79: "crew-escapes" (the design’s six rows), "crew-escapes-checks" (the check frames’ escapes), "crew-escapes-limit" (5,000 in all), "crew-escapes-empty", "crew-escapes-low", "crew-escapes-unreadable", "crew-escapes-stale", "crew-escapes-stored" (a person’s stored thresholds). */
  /** Lessons, KC5: "crew-lessons" (five sample lessons, one disabled and one over the cap), "crew-lessons-empty", "crew-lessons-unreadable", "crew-lessons-off" (the repository switch off, every lesson still readable). Skill proposals, KC9: "crew-proposals" (one open proposal from the design's three repeated failures), "crew-proposals-none", "crew-proposals-stale" (a source lesson turned off, so Approve is off), "crew-proposals-approved" (the skill exists and its lessons carry it), "crew-proposals-secret" (the scan refuses a text holding a key) and "crew-proposals-approve-fails" (skills-local refuses the write). */
  /** Review capacity and the coverage map, Crew Designs v77: "crew-capacity" (a limit of 8 saved, pausing off, every batch reviewed) and "crew-capacity-save-fails" (every save refused with 403 person_required). */
  /** Reviewer backtest, Crew Designs v23: "crew-backtest" (the estimate and a finished last run), "crew-backtest-report" (the finished run "run-4n"), "crew-backtest-stopped" (the stopped run "run-stopped"), "crew-backtest-kind-none" (a lane whose kind was not recorded), "crew-backtest-budget-none" (one engine has no budget), "crew-backtest-none" (the project never ran a backtest), "crew-backtest-failed" (a failed last run) and "crew-backtest-running" (a stored run in progress). */
  /** Crew Preview on (`crew`, `crew-<state>`): see `mock/crew.ts` for the states. Autopilot approvals: "crew-one-vendor". Autopilot by area: "crew-areas", "crew-areas-tripped", "crew-areas-ceiling-off", "crew-areas-recovering", "crew-areas-criteria-met", "crew-areas-restored", "crew-areas-lowdata", "crew-areas-empty", "crew-areas-stale". Guard, classes and routing: "crew-guard", "crew-guard-unknown", "crew-guard-full", "crew-guard-empty", "crew-classes", "crew-classes-invalid", "crew-classes-note", "crew-route-blocked", "crew-route-invalid", "crew-route-none". */
  /** Code Mode (dev and test stand-in for the service routes, mock/codemode.ts): the first thread with a search, a describe, a finished run, and a paused run with its approval. */
  | "codemode"
  | `crew${"" | `-${string}`}`
  /** The Pull requests page with its rows in every data state, and the variants a screen needs. */
  | "pulls"
  | "pulls-credits-on"
  | "pulls-details-failed"
  | "pulls-rate-limited"
  /** The first details read is answered in full, then the service pauses every detail field (as it does when GitHub's limit is spent) and names when it resumes, so a filter set on the details is already on when the pause comes (press Refresh): "pulls-limited" in ten minutes (pr-tb-filter-rate-limited), "pulls-limited-still" two seconds from every read, so the check at that time finds the limit still on (pr-tb-filter-still-limited). */
  | "pulls-limited"
  | "pulls-limited-still"
  | "pulls-signed-out"
  | "pulls-none"
  /** One row is Ready to merge (#137 approved, checks passing): its hover actions offer Merge. */
  | "pulls-ready"
  | "pulls-unreachable"
  /** The Usage page's Limits tab: Claude Code reads Not reported, Codex reports its windows, and the other providers have no route. */
  | "limits"
  | "limits-claude"
  | "limits-near"
  | "limits-reached"
  | "limits-signedout"
  | "limits-error"
  | "limits-providers"
  /** Settings › Browser: named profiles (one in use, one with a very long name); "browser-profiles-empty" has none. */
  | "browser-profiles"
  | "browser-profiles-empty"
  /** The first thread on Cursor: the first send answers 409 engine_notice_required, every later one 409 engine_mode_refused (use with #/preview/cursor-thread). */
  | "cursor-thread"
  /** A thread on Kiro, an engine the service says Orbyn cannot gate, for the unmanaged-thread screens: the one mode, the notice and its labels. */
  | "unmanaged-thread"
  /** Transcript import: a project default engine, so the sheet can preview; `transcript-imported` adds a thread made from a file. */
  | "transcript-import"
  | "transcript-import-no-default"
  | "transcript-imported";

type Subscriber = (event: WireThreadEvent) => void;

interface Failure {
  path: string;
  method?: string | undefined;
  remaining: number;
  status: number;
  code: string;
  message: string;
}

export interface MockOptions {
  /** Delay between scripted events. Zero runs them on the next tick. */
  stepMs?: number;
}

const err = (status: number, code: string, message: string, detail?: unknown): MockResponse => ({
  status,
  body: detail === undefined ? { code, message } : { code, message, detail },
});

const textModel = (id: string, label: string, extra: Partial<ModelInfo> = {}): ModelInfo => ({
  id, label, description: null, is_default: false, custom: false, options: [], ...extra,
});

/** The three engines K58 draws when the dev mock answers: a legacy fallback, a default model and no list. */
const TEXT_GENERATOR_SAMPLES: { engine: string; name: string; models: ModelInfo[] }[] = [
  { engine: "claude", name: "Claude Code", models: [textModel("haiku-5-5", "Haiku 5.5")] },
  { engine: "codex", name: "Codex", models: [
    textModel("gpt-5.2-mini", "GPT-5.2 mini"),
    textModel("gpt-5.2-codex", "GPT-5.2 Codex"),
    textModel("gpt-5.2-codex-max", "GPT-5.2 Codex Max", { is_default: true }),
    textModel("team-tuned-mini", "team-tuned-mini", { custom: true }),
  ] },
  { engine: "opencode", name: "OpenCode", models: [] },
  // Off in the default list (it is in `disabled_engines`): it lists a model, and none of them is small.
  { engine: "cursor", name: "Cursor", models: [textModel("composer-1", "Composer 1")] },
];

/** The engine's small model, recognised by family name as the service does. */
function smallTextModel(models: ModelInfo[]): string | null {
  for (const family of ["haiku", "luna", "mini", "flash", "small"]) {
    const found = models.find((model) => model.id.toLowerCase().includes(family));
    if (found) return found.id;
  }
  return null;
}

/** What one engine resolves to for one map, and why. */
function resolveTextModel(engine: string, models: ModelInfo[], map: Record<string, JsonValue>, legacy: JsonValue) {
  const named = typeof map[engine] === "string" && map[engine] !== "" ? String(map[engine]) : null;
  if (named !== null && models.some((model) => model.id === named)) return { chosen: named, resolved: named, reason: "chosen" as const };
  if (typeof legacy === "string" && legacy !== "" && models.some((model) => model.id === legacy)) return { chosen: named, resolved: legacy, reason: "legacy" as const };
  const small = smallTextModel(models);
  if (small !== null) return { chosen: named, resolved: small, reason: "small" as const };
  return { chosen: named, resolved: null, reason: null };
}

export function createMockApi(options: MockOptions = {}) {
  const stepMs = options.stepMs ?? 90;

  let scenario: Scenario = "default";
  let signedIn = false;
  let projects: Project[] = [];
  let threads: Thread[] = [];
  const explorerRoute = createExplorerMock(() => projects, () => threads);
  let approvals: Approval[] = [];
  let logs: Record<string, WireThreadEvent[]> = {};
  let engines: EngineStatus[] = [];
  /** What the service keeps of each install, update and removal, newest first, twenty at most. */
  const installs: Record<string, InstallOutcome[]> = {};
  const keepInstall = (outcome: InstallOutcome) => {
    installs[outcome.engine] = [outcome, ...(installs[outcome.engine] ?? [])].slice(0, 20);
  };
  let managed: WorkspaceStatus = fixtures.workspaceStatus([]);
  let failNextSetup = false;
  let updates: UpdateStatus = fixtures.updateStatus(2);
  let settings: ServerSettings = fixtures.serverSettings();
  /** What Delete now would remove, when a screen needs a different dry run than the default (`cleanupSummary`). */
  let cleanupPreviewOverride: CleanupPreview | null = null;
  let failStep: string | null = null;
  let queued: QueuedMessage[] = [];
  /** Files the editor saved, by thread and path, with the revision the next save must start from. */
  const savedFiles = new Map<string, { content: string; revision: string; count: number }>();
  let tasks: ScheduledTask[] = [];
  let cursorSends = 0;
  let browserProfiles: { id: string; name: string; in_use: boolean }[] = [];
  /** The thread each imported agent-history session became, by `engine:session_id`. */
  const importedHistory = new Map<string, string>();
  /** The thread each imported ACP-agent session became, and the ones deleted, by `engine:session_id`. */
  const importedNative = new Map<string, string>();
  const nativeDeleted = new Set<string>();
  /** What a test changed of the diagnostics the service reports (events, processes, database). */
  let diagnosticsPatch: Partial<Diagnostics> = {};
  /** Projects settings treats as checkouts of one repository, so a group selection has several members. */
  /** Programs the service found on the computer besides the engines it lists, as the provider list reports them. */
  let foundPrograms: ProviderEntry[] = [];
  /** What a test set for how many saved conversations an engine keeps, by engine. */
  const nativeCounts = new Map<string, { count: number; truncated: boolean }>();
  let groupedProjects: string[] = [];
  /** Tasks whose webhook has an address; the token itself is never kept, as in the service. */
  const hooks = new Set<string>(["task-hook"]);
  let failures: Failure[] = [];
  let clones: unknown[] = [];
  /** Why the next clone ends failed, or null while clones succeed. */
  let cloneError: string | null = null;
  /** The notes of the reviews sent from a pull request page, by number. */
  const reviewsSent = new Map<number, string[]>();
  /** The details reads since the scenario began: the limited scenarios answer the first in full and then hit the limit. */
  let detailsReads = 0;
  let delayMs = 0;
  let idCounter = 0;
  let preferenceValues: Record<string, JsonValue> = {};
  let preferenceOverrides: Record<string, Record<string, JsonValue>> = {};
  const preferenceDefaults: Record<string, JsonValue> = { ...preferenceCatalog, default_thread_env_mode: "local", branch_name_prefix: "feat",
    default_engine: "", default_model: "", default_effort: "", default_model_options: {}, fallback_model: "", disabled_engines: ["cursor", "kimi"],
    auto_thread_titles: true, default_permission_mode: "supervised", response_streaming_mode: "paragraph", pull_request_merge_method: "squash",
    source_control_writing_style: { mode: "conventional_commits", custom_instructions: "", follow_templates: true } };
  const subscribers = new Map<string, Set<Subscriber>>();
  const timers = new Set<ReturnType<typeof setTimeout>>();
  const nextId = (prefix: string) => `${prefix}-${++idCounter}`;
  const now = () => new Date(fixtures.T0 + idCounter * 1000).toISOString();

  const accountMock = createAccountMock();
  const crewMock = createCrewMock();
  const codeModeMock = createCodeModeMock();
  const capacityMock = createCapacityMock();
  const escapesMock = createEscapesMock();
  const lessonsMock = createLessonsMock();
  const proposalsMock = createSkillProposalsMock({ onApproved: (name, lessons) => lessonsMock.approveSkill(name, lessons) });
  const backtestMock = createBacktestMock();
  const backgroundMock = createBackgroundMock();
  const reviewControlsMock = createReviewControlsMock();
  const terminalsMock = createTerminalsMock();
  const questionsMock = createQuestionsMock();
  const foldersMock = createFoldersMock();
  /** What a test changes in the service's `/api/info` answer (the build channel, the version). */
  let infoPatch: Partial<ServerInfo> = {};
  /** The model lists a test gives an engine, in place of the sample ones. */
  let modelLists: Record<string, EngineModels> = {};
  const keybindingsMock = createKeybindingsMock();
  const secretsMock = createSecretsMock(() => projects.map((project) => ({ id: project.id, name: project.name })));

  /** Kiro, an ACP engine the service says Orbyn cannot gate, for the engine picker and unmanaged-thread screens. */
  const addKiro = (permission_modes: EngineStatus["capabilities"]["permission_modes"]) => {
    const cursor = engines.find((engine) => engine.id === "cursor");
    if (!cursor) return;
    engines.push({
      ...structuredClone(cursor),
      id: "kiro",
      name: "Kiro",
      tier: "acp",
      capabilities: { ...cursor.capabilities, managed: false, approvals: false, permission_modes },
      path: "/usr/local/bin/kiro-cli",
      version: null,
      auth: "unknown",
      installer: "unknown",
      latest: null,
      support: "unknown",
      update_command: null,
      manual_hint: null,
      sign_in_command: "kiro-cli login",
      updating: false,
      problem: null,
      crew_role: "reviewer_only",
    });
  };

  function reset(next: Scenario = "default") {
    for (const timer of timers) clearTimeout(timer);
    timers.clear();
    scenario = next;
    accountMock.reset(next === "account");
    crewMock.reset(next);
    codeModeMock.reset();
    capacityMock.reset(next);
    escapesMock.reset(next);
    lessonsMock.reset(next);
    proposalsMock.reset(next);
    backtestMock.reset(next);
    backgroundMock.reset(next);
    reviewControlsMock.reset(next);
    secretsMock.reset();
    terminalsMock.reset();
    questionsMock.reset();
    foldersMock.reset();
    keybindingsMock.reset();
    cleanupPreviewOverride = null;
    signedIn = false;
    infoPatch = {};
    modelLists = {};
    idCounter = 0;
    preferenceValues = {};
    preferenceOverrides = {};
    failures = [];
    clones = [];
    importedHistory.clear();
    importedNative.clear();
    nativeDeleted.clear();
    diagnosticsPatch = {};
    groupedProjects = [];
    nativeCounts.clear();
    foundPrograms = [];
    cloneError = null;
    reviewsSent.clear();
    detailsReads = 0;
    delayMs = 0;
    const blank = next === "empty" || next === "first-run" || next === "no-skills";
    projects = blank ? [] : structuredClone(fixtures.projects);
    // Autopilot by area is drawn for one repository (the design has a separate frame for several): its scenarios keep the first project.
    if (next.startsWith("crew-areas")) projects = projects.slice(0, 1);
    threads = blank ? [] : fixtures.threads();
    if (next === "settings-checkouts") {
      projects = [...projects, { id: "p-app-fix", name: "orbyn-engineering-app (fix-login)", path: "/Users/dev/code/orbyn-engineering-app-wt/fix-login",
        trusted: true, is_git: true, managed: false, scratch: false, created_at: fixtures.projects[0]!.created_at }];
    }
    if (next === "transcript-imported") threads = [...threads, fixtures.importedThread()];
    approvals = blank ? [] : fixtures.pendingApprovals();
    tasks = blank ? [] : fixtures.scheduledTasks();
    cursorSends = 0;
    if (next === "cursor-thread") threads = threads.map((t) => (t.id === "t-bg-stopped" ? { ...t, engine: "cursor" } : t));
    browserProfiles = next === "browser-profiles"
      ? [
          { id: "bp-work", name: "Work", in_use: true },
          { id: "bp-personal", name: "Personal sign-ins", in_use: false },
          { id: "bp-long", name: "Staging environment for the checkout flow with the long customer name", in_use: false },
        ]
      : [];
    logs = blank ? {} : fixtures.logs();
    if (next === "transcript-imported") logs["t-imported"] = fixtures.importedLog();
    if (next === "reference") {
      const state = fixtures.referenceState(Date.now());
      projects = state.projects;
      threads = state.threads;
      logs = state.logs;
      approvals = [];
    }
    if (next === "codemode") {
      // Code Mode (docs/code-mode.md): the first thread shows a search, a describe, a finished run and a paused one with its approval.
      logs["t-approval"] = fixtures.codeModeLog();
      approvals = [fixtures.codeModeApproval()];
      codeModeMock.seedDemo();
    }
    managed = fixtures.workspaceStatus(projects);
    failNextSetup = false;
    updates = fixtures.updateStatus(2);
    if (next === "updates-unconfigured") {
      updates.app = { ...updates.app, latest: null, notes: null, can_apply: false, unconfigured: true };
    }
    settings = fixtures.serverSettings();
    failStep = null;
    if (next === "no-skills") managed = fixtures.workspaceStatus([], false);
    engines = structuredClone([...fixtures.engines, ...instanceEngines(fixtures.engines)]);
    if (next.startsWith("limits")) {
      const keep = next === "limits-providers" ? ["gemini-acp", "aider"] : next === "limits-claude" ? ["claude"] : next === "limits-error" ? ["codex", "gemini-acp"] : ["claude", "codex"];
      engines = engines.filter((engine) => keep.includes(engine.id));
      for (const engine of engines) if (engine.id === "codex" && next !== "limits-signedout") engine.auth = "logged_in";
    }
    if (next === "transcript-import" || next === "transcript-imported") {
      preferenceValues = { ...preferenceValues, default_engine: "claude", default_model: "", default_effort: "", default_model_options: {}, disabled_engines: [] };
    }
    if (next === "transcript-import-no-default") preferenceValues = { ...preferenceValues, disabled_engines: [] };
    // The engine picker's Unmanaged label, on an ACP engine the service says it cannot gate.
    if (next === "crew-engine-picker") {
      addKiro(["supervised", "auto", "full_access"]);
      preferenceValues = {
        ...preferenceValues,
        default_engine: "kiro",
        default_model: "",
        default_effort: "",
        default_model_options: {},
        disabled_engines: [],
      };
    }
    // The unmanaged thread's one mode, notice and labels, on a thread the service says Orbyn cannot gate.
    if (next === "unmanaged-thread") {
      addKiro(["no_approvals"]);
      threads = threads.map((thread) => thread.id === "t-empty"
        ? { ...thread, project_id: "p-app", engine: "kiro", model: "sonnet", lifecycle: { ...(thread.lifecycle ?? fixtures.lifecycle()), permission_mode: "auto" } }
        : thread);
    }
    if (!blank) {
      preferenceValues = { ...preferenceValues, text_generation_model: "haiku-5-5", provider_instances: structuredClone(providerInstances), custom_models: structuredClone(customModels) };
      // The project scope's own Codex choice, as the textgen-list frame draws it.
      const firstProject = projects[0]?.id;
      if (firstProject) preferenceOverrides[firstProject] = { text_generation_models: { codex: "gpt-5.2-codex" } };
    }
  }
  reset();

  function append(threadId: string, runId: string | null, event: AgUiEvent): WireThreadEvent {
    const log = (logs[threadId] ??= []);
    const wire: WireThreadEvent = {
      seq: (log.at(-1)?.seq ?? 0) + 1,
      thread_id: threadId,
      run_id: runId,
      ts: new Date(fixtures.T0 + (log.length + 1) * 1000).toISOString(),
      event,
    };
    log.push(wire);
    for (const subscriber of subscribers.get(threadId) ?? []) subscriber(wire);
    return wire;
  }

  function setRun(threadId: string, patch: Partial<Thread>) {
    threads = threads.map((thread) =>
      thread.id === threadId ? { ...thread, ...patch, updated_at: now() } : thread,
    );
  }

  function play(steps: (() => void)[]) {
    let at = 0;
    const tick = () => {
      const step = steps[at];
      at += 1;
      if (!step) return;
      step();
      const timer = setTimeout(tick, stepMs);
      timers.add(timer);
    };
    const timer = setTimeout(tick, stepMs);
    timers.add(timer);
  }

  function startRun(thread: Thread, prompt: string): MockResponse {
    const project = projects.find((p) => p.id === thread.project_id);
    if (project && !project.trusted) {
      return err(403, "folder_untrusted", "Trust this folder before running an engine in it.");
    }
    if (thread.active_run) return err(409, "thread_busy", "This thread already has an active run.");
    if (scenario === "cursor-thread" && thread.engine === "cursor") {
      cursorSends += 1;
      return cursorSends === 1
        ? err(409, "engine_notice_required", "Cursor does not ask before acting.")
        : err(409, "engine_mode_refused", "Cursor cannot run in Supervised mode.");
    }
    // The notice is the only unmanaged-thread state the service sends for a non-Cursor engine today; the mode refusal is planned.
    if (scenario === "unmanaged-thread" && thread.engine === "kiro") {
      return err(409, "engine_notice_required", "Orbyn cannot ask before Kiro acts.");
    }
    const runId = nextId("run");
    const run = {
      id: runId,
      thread_id: thread.id,
      state: "running" as const,
      error: null,
      started_at: now(),
      ended_at: null,
    };
    setRun(thread.id, { active_run: run, last_run_state: "running" });
    const message = nextId("m");
    const live = () => threads.find((t) => t.id === thread.id)?.active_run?.id === runId;
    const steps: (() => void)[] = fixtures
      .replyEvents(runId, thread.id, prompt, message)
      .map((event) => () => {
        if (live()) append(thread.id, runId, event);
      });
    const answer = `a-${message}`;
    steps.push(() => {
      if (live()) append(thread.id, runId, { type: "TEXT_MESSAGE_START", messageId: answer, role: "assistant" });
    });
    for (const word of fixtures.replyText.match(/\S+\s*/g) ?? []) {
      steps.push(() => {
        if (live()) append(thread.id, runId, { type: "TEXT_MESSAGE_CONTENT", messageId: answer, delta: word });
      });
    }
    steps.push(() => {
      if (!live()) return;
      append(thread.id, runId, { type: "TEXT_MESSAGE_END", messageId: answer });
      append(thread.id, runId, {
        type: "RUN_FINISHED",
        threadId: thread.id,
        runId,
        outcome: { type: "success" },
      });
      setRun(thread.id, { active_run: null, last_run_state: "completed" });
    });
    play(steps);
    return { status: 202, body: run };
  }

  function decide(approval: Approval, allow: boolean): MockResponse {
    if (approval.state !== "pending") return err(409, "already_decided", `Already ${approval.state}.`);
    const decided: Approval = {
      ...approval,
      state: allow ? "allowed" : "denied",
      decided_by: "person",
      decided_at: now(),
    };
    approvals = approvals.filter((a) => a.id !== approval.id);
    append(approval.thread_id, approval.run_id, fixtures.approvalEvent(decided));
    if (!approvals.some((a) => a.thread_id === approval.thread_id)) {
      const thread = threads.find((t) => t.id === approval.thread_id);
      if (thread?.active_run) {
        const state = "running" as const;
        setRun(thread.id, { active_run: { ...thread.active_run, state }, last_run_state: state });
        const runId = thread.active_run.id;
        const id = nextId("m");
        const finish = () => {
          if (threads.find((t) => t.id === thread.id)?.active_run?.id !== runId) return;
          append(thread.id, runId, { type: "TEXT_MESSAGE_START", messageId: id, role: "assistant" });
          append(thread.id, runId, {
            type: "TEXT_MESSAGE_CONTENT",
            messageId: id,
            delta: allow ? "Approved. Finished the work." : "Denied, so I stopped there.",
          });
          append(thread.id, runId, { type: "TEXT_MESSAGE_END", messageId: id });
          append(thread.id, runId, { type: "RUN_FINISHED", threadId: thread.id, runId, outcome: { type: "success" } });
          setRun(thread.id, { active_run: null, last_run_state: "completed" });
        };
        play([finish]);
      }
    }
    return { status: 200, body: decided };
  }

  /** Project rows follow the live list; everything else is the fixture state. */
  const workspaceBody = (): WorkspaceStatus => {
    const rows = fixtures.workspaceStatus(projects).targets;
    return {
      ...managed,
      targets: managed.configured ? rows : [],
    };
  };

  function setup(): MockResponse {
    if (failNextSetup) {
      failNextSetup = false;
      return err(502, "clone_failed", "The library remote couldn't be reached.");
    }
    managed = fixtures.workspaceStatus(projects);
    return { status: 200, body: workspaceBody() };
  }

  const activeRuns = () => threads.filter((t) => t.active_run).length;

  /** The server sends the reason; the client only shows it. */
  const updatesBody = (): UpdateStatus => {
    const runs = activeRuns();
    const dirty = managed.library?.dirty ?? false;
    const runsReason = runs > 0 ? (runs === 1 ? "1 run is active." : `${runs} runs are active.`) : null;
    const libraryReason = dirty ? "The library has local changes." : runsReason;
    return {
      ...updates,
      engines: updates.engines.map((engine) => ({ ...engine, blocked_reason: runsReason })),
      library: { ...updates.library, dirty, can_apply: libraryReason === null, blocked_reason: libraryReason },
      app: {
        ...updates.app,
        // The desktop app says it in the service's words (operations/updates.rs); the other modes keep the count.
        blocked_reason: updates.app.mode === "server" ? updates.app.blocked_reason
          : updates.app.mode === "desktop" ? (runs > 0 ? "Finish or stop active runs first." : updates.app.blocked_reason) : runsReason,
      },
    };
  };

  function finishStep(target: string) {
    if (target.startsWith("engine:")) {
      updates = {
        ...updates,
        engines: updates.engines.map((engine) =>
          `engine:${engine.id}` === target ? { ...engine, current: engine.latest } : engine,
        ),
      };
      const engine = engines.find((e) => `engine:${e.id}` === target);
      if (engine) engine.version = engine.latest;
    } else if (target === "library") {
      updates = {
        ...updates,
        library: { ...updates.library, current: updates.library.latest, behind: 0, incoming: [] },
      };
      if (managed.library) {
        managed = {
          ...managed,
          library: {
            ...managed.library,
            previous_revision: managed.library.revision,
            revision: updates.library.latest ?? managed.library.revision,
            behind: 0,
          },
        };
      }
    } else if (target === "app") {
      updates = { ...updates, app: { ...updates.app, current: updates.app.latest ?? updates.app.current } };
    }
  }

  function applyAll(): MockResponse {
    if (activeRuns() > 0) return err(409, "runs_active", "Runs are active.");
    const targets = [
      ...updates.engines
        .filter((e) => e.can_apply && e.latest !== e.current)
        .map((e) => `engine:${e.id}`),
      ...(updates.library.behind > 0 ? ["library"] : []),
      ...(updates.app.can_apply && updates.app.latest !== updates.app.current ? ["app"] : []),
    ];
    const result: ApplyAllResult = { steps: [] };
    for (const target of targets) {
      if (failStep === target) {
        failStep = null;
        result.steps.push({ target, ok: false, message: "The installer exited with status 1." });
        break;
      }
      finishStep(target);
      result.steps.push({ target, ok: true, message: null });
    }
    return { status: 200, body: result };
  }

  function newProject(path: string, trusted = false): Project {
    const project: Project = {
      id: nextId("p"),
      name: path.split("/").at(-1)?.replace(/\.git$/, "") ?? path,
      path,
      trusted,
      is_git: true,
      managed: false,
      scratch: false,
      created_at: now(),
    };
    projects = [...projects, project];
    return project;
  }

  /** The repository a checkout belongs to; every other project is its own group. */
  const repositoryOf = (project: Project) => (scenario === "settings-checkouts" && project.id.startsWith("p-app") ? "orbyn-engineering-app" : null);
  /** The catalog key of the group a project is in (`repo:` for checkouts of one repository, `mock:` for a project of its own). */
  const groupKeyOf = (projectId: string) => {
    const project = projects.find((entry) => entry.id === projectId);
    const repository = project ? repositoryOf(project) : null;
    return repository ? `repo:${repository}` : `mock:${projectId}`;
  };

  function scopeCatalog(scope: ProjectScopesQuery): ProjectScopeCatalog {
    const member = (project: Project) => ({
      project_id: project.id, physical_key: `mock-physical:${project.id}`, name: project.name, path: project.path,
      repository_key: repositoryOf(project), repository_label: repositoryOf(project), repository_relative_path: "",
      grouping_mode: repositoryOf(project) ? "repository" as const : "separate" as const,
      metadata_complete: true, unavailable: null,
    });
    const listed = projects.filter((project) => !project.scratch && project.path !== managed.library?.path);
    // Checkouts of one repository are one group; so are the projects a test asked to be grouped (`groupProjects`).
    const asked = new Set(groupedProjects);
    const togetherAll = listed.filter((project) => asked.has(project.id));
    // A group that lost members down to one is that project's own group again.
    const together = togetherAll.length > 1 ? togetherAll : [];
    const grouped = new Map<string, Project[]>();
    for (const project of listed.filter((entry) => !together.includes(entry))) {
      const repository = repositoryOf(project);
      const key = repository ? `repo:${repository}` : `mock:${project.id}`;
      grouped.set(key, [...(grouped.get(key) ?? []), project]);
    }
    const groups = [
      ...[...grouped].map(([key, members]) => ({ key, label: repositoryOf(members[0]!) ?? members[0]!.name, members: members.map(member) })),
      ...(together.length > 1 ? [{ key: `mock:group:${together.map((project) => project.id).join("+")}`, label: together[0]?.name ?? "", members: together.map(member) }] : []),
    ];
    const selection = scope.selection;
    const selected = selection && selection.kind !== "all_projects" ? groups.find((group) => group.key === selection.group_key) : undefined;
    const resolved: ProjectScopeCatalog["selection"] = !selection || selection.kind === "all_projects" ? { kind: "all_projects" }
      : !selected ? { kind: "unavailable", reason: "project_missing" }
      : selection.kind === "checkout" ? selected.members.some((member) => member.project_id === selection.project_id)
        ? selection : { kind: "unavailable", reason: "checkout_missing" }
      : { kind: "project_group", group_key: selected.key, project_ids: selected.members.map((member) => member.project_id) };
    return { version: JSON.stringify(groups), groups, metadata_complete: true, selection: resolved };
  }

  function scopedPreferences(scope: ProjectScopesQuery, keys: string[]): ScopedPreferencesResult {
    const catalog = scopeCatalog(scope);
    const selected = catalog.selection;
    const ids = selected.kind === "project_group" ? selected.project_ids : selected.kind === "checkout" ? [selected.project_id] : [];
    return { catalog, rows: keys.filter((key) => Object.hasOwn(preferenceDefaults, key)).map((key) => {
      const fallback = preferenceDefaults[key] ?? null;
      const environment = { key, value: preferenceValues[key] ?? fallback, default: fallback,
        source: Object.hasOwn(preferenceValues, key) ? "global" : "default", project_scoped: projectPreferenceKeys.has(key) };
      const members = ids.map((id) => ({ project_id: id, name: projects.find((project) => project.id === id)?.name ?? id,
        view: { ...environment, value: preferenceOverrides[id]?.[key] ?? environment.value,
          source: Object.hasOwn(preferenceOverrides[id] ?? {}, key) ? "project" : environment.source } }));
      const value = members[0]?.view.value ?? environment.value;
      return { key, value, mixed: members.some((member) => JSON.stringify(member.view.value) !== JSON.stringify(value)),
        default: fallback, project_scoped: projectPreferenceKeys.has(key), revision: JSON.stringify([environment, members]), environment, members,
        overrides: projects.filter((project) => Object.hasOwn(preferenceOverrides[project.id] ?? {}, key)).map((project) => ({
          project_id: project.id, name: project.name, value: preferenceOverrides[project.id]?.[key] ?? null,
        })) };
    }) };
  }

  /** The legacy preference view list, as GET /api/preferences and the two write routes answer it. */
  function preferenceViews(scope: ProjectScopesQuery) {
    return scopedPreferences(scope, Object.keys(preferenceDefaults)).rows.map((row) => row.members[0]?.view ?? row.environment);
  }

  /** The direct preferences routes merge a top-level object key (the text generator map) and replace the rest. */
  function writePreferences(projectId: string | null, values: Record<string, JsonValue>) {
    const target = projectId === null ? preferenceValues : (preferenceOverrides[projectId] ??= {});
    for (const [key, value] of Object.entries(values)) {
      const kept = target[key];
      target[key] = value !== null && typeof value === "object" && !Array.isArray(value) && kept !== null && typeof kept === "object" && !Array.isArray(kept)
        ? { ...kept, ...value } : value;
    }
  }

  /** The failure a test queued for this path, if one is still waiting, used up by one request. */
  function takeFailure(pathname: string, method?: string): MockResponse | null {
    const failure = failures.find((f) => pathname.startsWith(f.path) && f.remaining > 0 && (f.method === undefined || method === undefined || f.method === method));
    if (!failure) return null;
    failure.remaining -= 1;
    return err(failure.status, failure.code, failure.message);
  }

  function route(method: string, path: string, query: URLSearchParams, body: unknown): MockResponse {
    const seg = path.split("/").filter(Boolean).map(decodeURIComponent);
    const key = `${method} /${seg.slice(0, 2).join("/")}`;
    const data = (body ?? {}) as Record<string, unknown>;

    if (scenario === "remote" && !signedIn && !path.startsWith("/auth")) {
      return err(401, "unauthorized", "Sign in with the access key.");
    }

    const explored = explorerRoute(method, seg, query);
    if (explored) return explored;
    const terminal = terminalsMock.handle(method, seg, body);
    if (terminal) return terminal;
    const question = questionsMock.handle(method, seg, query, body);
    if (question) return question;

    if (method === "POST" && seg[1] === "projects" && seg[3] === "pulls" && seg[5] === "review" && scenario.startsWith("pulls") && fixtures.pageStates[Number(seg[4])]) {
      reviewsSent.set(Number(seg[4]), [...(reviewsSent.get(Number(seg[4])) ?? []), String(data.body ?? "")]);
      return { status: 204 };
    }
    if (method === "GET" && seg[1] === "projects" && seg[3] === "pulls" && seg[4] && scenario.startsWith("pulls") && fixtures.pageStates[Number(seg[4])]) {
      const state = fixtures.pageStates[Number(seg[4])]!;
      if (!seg[5]) {
        return {
          status: 200,
          body: {
            pull: state.pull,
            body: "Login now returns people to the page they came from when `next` points inside the app, and to `/` otherwise.",
            checks: [],
            comments: [],
            mergeable: state.mergeable,
            reviewers: state.reviewers,
            head_sha: "abc1234",
            base_sha: "def5678",
            additions: 4,
            deletions: 3,
            changed_files: 2,
            maintainer_can_modify: null,
            auto_merge: false,
          },
        };
      }
      if (seg[5] === "checks") return { status: 200, body: fixtures.pageChecks(Number(seg[4])) };
      if (seg[5] === "files") return { status: 200, body: fixtures.pageFiles() };
      // The conversation: three comments, and each review sent from this page after them.
      if (seg[5] === "timeline") {
        const comments = [1, 2, 3].map((id) => ({ id: `c${id}`, kind: "commented", actor: "sam", created_at: null, text: `Comment ${id}` }));
        const sent = (reviewsSent.get(Number(seg[4])) ?? []).map((text, at) => ({ id: `r${at}`, kind: "reviewed", actor: "you", created_at: null, text }));
        return { status: 200, body: [...comments, ...sent] };
      }
      if (seg[5] === "comments") return { status: 200, body: { comments: [], threads: [] } };
      if (seg[5] === "stack") return { status: 200, body: { layers: [], position: 0 } };
    }
    if (method === "GET" && seg[1] === "projects" && seg[3] === "pulls" && seg[4]) {
      const pull = fixtures.pulls().find((entry) => String(entry.pull.number) === seg[4])?.pull;
      if (!pull) return err(404, "not_found", "Pull request not found.");
      if (!seg[5]) {
        return {
          status: 200,
          body: {
            pull,
            body: "Adds a timeout to approvals, so a forgotten request no longer holds a run open.",
            checks: [],
            comments: [],
            mergeable: "mergeable",
            reviewers: ["sam"],
            head_sha: "abc1234",
            base_sha: "def5678",
            additions: 48,
            deletions: 12,
            changed_files: 3,
            maintainer_can_modify: null,
            auto_merge: false,
          },
        };
      }
      if (seg[5] === "checks" || seg[5] === "timeline") return { status: 200, body: [] };
      if (seg[5] === "comments") return { status: 200, body: fixtures.pullConversation() };
      if (seg[5] === "stack") return { status: 200, body: { layers: [], position: 0 } };
    }

    // The pull requests linked to a thread, and a refresh of them: arrays, as the side panel maps them.
    if (method === "GET" && seg[1] === "threads" && seg[3] === "pulls" && !seg[4]) {
      return { status: 200, body: fixtures.pulls().map((entry) => entry.pull) };
    }
    if (method === "POST" && seg[1] === "threads" && seg[3] === "pulls" && seg[4] === "refresh") {
      return { status: 200, body: fixtures.pulls().map((entry) => entry.pull) };
    }

    if (method === "GET" && seg[1] === "threads" && seg[3] === "export" && !seg[4]) {
      const thread = threads.find((entry) => entry.id === seg[2]);
      if (!thread) return err(404, "not_found", "Thread not found.");
      return {
        status: 200,
        body: {
          format: "orbyn-transcript",
          version: 1,
          exported_at: now(),
          thread: { title: thread.title, engine: thread.engine, model: thread.model, created_at: thread.created_at },
          messages: [
            { role: "user", ts: thread.created_at, text: "The logout page redirects back to itself in a loop." },
            { role: "assistant", ts: thread.updated_at, text: "The cookie is cleared after the redirect has already fired." },
          ],
          omitted_earlier: 0,
        },
      };
    }

    if (method === "POST" && seg[1] === "threads" && seg[2] === "import") {
      const project = projects.find((entry) => entry.id === query.get("project_id"));
      if (!project) return err(404, "not_found", "Project not found.");
      const name = query.get("name") ?? "transcript.orbyn.json";
      if (/huge/i.test(name)) return err(413, "file_too_large", "A transcript is at most 2 MiB.");
      if (/newer/i.test(name)) return err(422, "transcript_too_new", "This transcript was made by a newer Orbyn.");
      if (/notes/i.test(name)) return err(422, "transcript_unreadable", "This file is not an Orbyn transcript.");
      if (/nokey/i.test(name)) return err(403, "person_required", "This action is only available in the desktop app.");
      if (/engine-gone/i.test(name)) return err(404, "engine_unavailable", "That engine is not available here. Choose another.");
      const document = (body ?? {}) as Record<string, unknown>;
      const sourceThread = typeof document.thread === "object" && document.thread !== null ? (document.thread as Record<string, unknown>) : {};
      const messages = Array.isArray(document.messages) ? document.messages : [];
      const engineId = query.get("engine") ?? (typeof sourceThread.engine === "string" ? sourceThread.engine : "");
      const known = engines.find((entry) => entry.id === engineId);
      if (engineId === "" || !known || known.installed === false) {
        return err(404, "engine_unavailable", "That engine is not available here. Choose another.");
      }
      const namedTitle = typeof sourceThread.title === "string" && sourceThread.title !== "" ? sourceThread.title : null;
      const title = namedTitle ?? (name.replace(/\.orbyn\.json$/i, "").replace(/[-_]+/g, " ") || "Imported transcript");
      const droppedSample = /dropped/i.test(name);
      const problems = droppedSample
        ? [
            { kind: "message", index: 7, code: "unknown_role" },
            { kind: "message", index: 31, code: "text_empty" },
            { kind: "message", index: 96, code: "not_an_object" },
          ]
        : [];
      const common = {
        title,
        message_count: droppedSample ? 211 : messages.length,
        dropped: problems.length,
        problems,
      };
      if (query.get("dry_run") === "true") return { status: 200, body: { dry_run: true, thread: null, ...common } };
      const thread: Thread = {
        scratch: false,
        id: nextId("t"),
        project_id: project.id,
        title,
        engine: engineId,
        model: null,
        cwd: project.path,
        worktree: null,
        skill_profile: null,
        active_run: null,
        last_run_state: null,
        created_at: now(),
        updated_at: now(),
        source: "orbyn-transcript",
        imported_at: now(),
      };
      threads = [...threads, thread];
      return { status: 201, body: { dry_run: false, thread, ...common } };
    }

    switch (key) {
      case "GET /api/source-control":
        if (seg[2] === "repository-star") return { status: 200, body: { starred: true } };
        break;
      case "GET /api/updates":
        return { status: 200, body: updatesBody() };
      case "GET /api/tasks":
        if (seg.length === 4 && seg[3] === "deliveries") return { status: 200, body: fixtures.webhookDeliveries(seg[2] ?? "") };
        if (seg.length === 4 && seg[3] === "runs") return tasks.some((entry) => entry.id === seg[2]) ? { status: 200, body: fixtures.taskRuns(seg[2] ?? "") } : err(404, "not_found", "Task not found.");
        if (seg.length === 4 && seg[3] === "webhook") return { status: 200, body: { configured: hooks.has(seg[2] ?? ""), path_prefix: `/hooks/${seg[2]}/` } };
        if (seg.length !== 2) break;
        // Copy on read: callers hold this snapshot until their next reload.
        return { status: 200, body: structuredClone(tasks) };
      case "POST /api/tasks": {
        const task = tasks.find((entry) => entry.id === seg[2]);
        if (!task) return err(404, "not_found", "Task not found.");
        if (seg[3] === "enabled") {
          task.enabled = data.enabled === true;
          task.next_run_at = task.enabled ? fixtures.iso(600) : null;
          return { status: 200, body: task };
        }
        if (seg[3] === "webhook") {
          const token = `tok${++idCounter}x9f`;
          hooks.add(task.id);
          return { status: 200, body: { path: `/hooks/${task.id}/${token}`, token } };
        }
        if (seg[3] === "run") {
          task.last_status = "started";
          task.last_run_at = now();
          return { status: 202 };
        }
        break;
      }
      case "DELETE /api/tasks":
        tasks = tasks.filter((entry) => entry.id !== seg[2]);
        return { status: 204 };
      case "POST /api/updates":
        if (seg[2] === "check") {
          updates = { ...updates, checked_at: new Date().toISOString() };
          return { status: 200, body: updatesBody() };
        }
        if (seg[2] === "apply-all") return applyAll();
        if (seg[2] === "app") {
          if (updates.app.release_id !== data.release_id || updates.app.operation !== data.operation) {
            return err(422, "update_unavailable", "The available update changed. Review the new version before downloading.");
          }
          if (activeRuns() > 0) return err(409, "runs_active", "Finish or stop active runs first.");
          if (seg[3] === "download" && updates.app.phase === "available") {
            // The deterministic fake models a verified download; consent never installs.
            updates = { ...updates, app: { ...updates.app, phase: "ready", operation: updates.app.operation + 1 } };
            return { status: 202 };
          }
          if (seg[3] === "apply" && updates.app.phase === "ready") {
            finishStep("app");
            updates = { ...updates, app: { ...updates.app, phase: "idle" } };
            return { status: 202 };
          }
          return err(422, "update_unavailable", "No update is ready.");
        }
        break;
      case "GET /api/preferences": {
        const projectId = query.get("project_id");
        const scope: ProjectScopesQuery = projectId ? { selection: { kind: "checkout", group_key: groupKeyOf(projectId), project_id: projectId } } : {};
        if (scopedPreferences(scope, []).catalog.selection.kind === "unavailable") return err(404, "not_found", "Project not found.");
        const rows = preferenceViews(scope);
        // The credits setting on, for the read-only merge dialog frame (pr-detail-merge-dialog-credits-readonly-on).
        if (scenario === "pulls-credits-on") {
          return { status: 200, body: rows.map((view) => (view.key === "remove_agent_credits_on_merge" ? { ...view, value: true } : view)) };
        }
        return { status: 200, body: rows };
      }
      case "PUT /api/preferences": {
        writePreferences(null, (data.values ?? {}) as Record<string, JsonValue>);
        return { status: 200, body: preferenceViews({}) };
      }
      case "GET /api/text-generators": {
        const projectId = query.get("project");
        if (projectId !== null && !projects.some((project) => project.id === projectId)) return err(404, "not_found", "Project not found.");
        const globalMap = (preferenceValues.text_generation_models ?? {}) as Record<string, JsonValue>;
        const projectMap = projectId === null ? {} : (preferenceOverrides[projectId]?.text_generation_models ?? {}) as Record<string, JsonValue>;
        const globalLegacy = preferenceValues.text_generation_model ?? "";
        const projectLegacy = projectId === null ? globalLegacy : preferenceOverrides[projectId]?.text_generation_model ?? globalLegacy;
        // Only the engines that are on for the project, as the service lists them.
        const disabled = (preferenceOverrides[projectId ?? ""]?.disabled_engines ?? preferenceValues.disabled_engines ?? preferenceDefaults.disabled_engines) as string[];
        const engines: TextGeneratorEngine[] = TEXT_GENERATOR_SAMPLES.filter((sample) => !disabled.includes(sample.engine)).map((sample) => {
          const global = resolveTextModel(sample.engine, sample.models, globalMap, globalLegacy);
          return {
            engine: sample.engine, name: sample.name, models: structuredClone(sample.models),
            chosen: global.chosen, resolved: global.resolved, reason: global.reason,
            ...(projectId === null ? {} : {
              project: (() => {
                const choice = resolveTextModel(sample.engine, sample.models, { ...globalMap, ...projectMap }, projectLegacy);
                return { project_id: projectId, chosen: choice.chosen, own: Object.hasOwn(projectMap, sample.engine), resolved: choice.resolved, reason: choice.reason };
              })(),
            }),
          };
        });
        return { status: 200, body: { engines } satisfies TextGenerators };
      }
      case "PUT /api/projects": {
        if (seg[3] !== "preferences") break;
        const projectId = seg[2] ?? "";
        if (!projects.some((project) => project.id === projectId)) return err(404, "not_found", "Project not found.");
        writePreferences(projectId, (data.values ?? {}) as Record<string, JsonValue>);
        return { status: 200, body: preferenceViews({ selection: { kind: "checkout", group_key: groupKeyOf(projectId), project_id: projectId } }) };
      }
      case "POST /api/settings":
        if (seg[2] === "project-scopes") return { status: 200, body: scopeCatalog(data as ProjectScopesQuery) };
        if (seg[2] === "preferences" && seg[3] === "query") {
          if (!data.scope) return err(400, "invalid_input", "Choose a settings scope.");
          const result = scopedPreferences(data.scope as ProjectScopesQuery, (data.keys ?? []) as string[]);
          return result.catalog.selection.kind === "unavailable"
            ? err(409, "settings_scope_unavailable", "Choose an available scope.")
            : { status: 200, body: result };
        }
        break;
      case "GET /api/history": {
        // As the service reads them: a session whose folder was added since sits under that project (importable once it is trusted), and an imported one names its thread.
        const scan = historyScan(Date.now());
        return { status: 200, body: { ...scan, sessions: scan.sessions.map((session) => {
          const project = session.project_id === null ? projects.find((entry) => entry.path === session.cwd) : undefined;
          const placed = project ? { ...session, project_id: project.id, importable: project.trusted } : session;
          return { ...placed, thread_id: importedHistory.get(`${session.engine}:${session.session_id}`) ?? null };
        }) } };
      }
      case "POST /api/history": {
        const thread_id = nextId("t");
        importedHistory.set(`${String(data.engine)}:${String(data.session_id)}`, thread_id);
        return { status: 200, body: { thread_id, imported: true } };
      }
      case "GET /api/acp":
        return { status: 200, body: acpRegistry };
      // Installing or removing a registry agent: answered, but the mock keeps its engines as they are.
      case "POST /api/acp":
        return acpRegistry.some((entry) => entry.id === seg[3]) ? { status: 204 } : err(404, "not_found", "That agent is not in the registry.");
      case "GET /api/diagnostics":
        return { status: 200, body: { ...diagnostics(Date.now()), ...diagnosticsPatch } };
      case "POST /api/diagnostics":
        return seg[2] === "otlp" ? { status: 200, body: { ok: true, status: 200, message: null } } : { status: 204 };
      // No desktop app is attached to this service.
      case "GET /api/shell":
        return { status: 200, body: { attached: false, windows: 0, facts: { cli: null, permissions: [] } } satisfies ShellStatus };
      case "GET /api/storage":
        if (seg[2] === "cleanup" && seg[3] === "preview") return { status: 200, body: cleanupPreviewOverride ?? cleanupSummary() };
        return { status: 200, body: null };
      case "POST /api/storage":
        return { status: 200, body: query.get("dry_run") === "true" ? cleanupPreview(Date.now()) : { ...cleanupPreview(Date.now()), dry_run: false } };
      case "GET /api/settings":
        return { status: 200, body: settings };
      // Where a kept attachment came from (docs/snapshots.md): the service answers for a Local image route.
      case "PUT /api/attachments":
        if (seg[3] !== "source") break;
        return { status: 204 };
      // The editor's save: it writes only over the revision it started from, as the service does.
      case "PUT /api/threads": {
        if (seg[3] !== "files" || seg[4] !== "content") break;
        const { path, content, base_revision } = body as { path: string; content: string; base_revision: string | null };
        const key = `${seg[2]}:${path}`;
        const current = savedFiles.get(key)?.revision ?? `mock-${seg[2]}-${path}`;
        if (base_revision !== current) return err(409, "file_conflict", "The file changed on disk.", { revision: current });
        const revision = `mock-${(savedFiles.get(key)?.count ?? 0) + 1}`;
        savedFiles.set(key, { content, revision, count: (savedFiles.get(key)?.count ?? 0) + 1 });
        return { status: 200, body: { path, content, revision, size: content.length, binary: false, truncated: false } };
      }
      case "PUT /api/settings":
        if (seg[2] === "preferences") {
          const change = data as unknown as ScopedPreferencesChange;
          const before = scopedPreferences(change.scope, Object.keys(change.edits));
          if (before.catalog.selection.kind === "unavailable") return err(409, "settings_scope_unavailable", "Choose an available scope.");
          if (change.membership_version !== before.catalog.version || before.rows.some((row) => change.if_match[row.key] !== row.revision)) {
            return err(409, "settings_conflict", "Settings changed. Reload before saving.");
          }
          for (const row of before.rows) {
            const edit = change.edits[row.key];
            if (!edit) continue;
            const targets = before.catalog.selection.kind === "all_projects" ? [null] : row.members.map((member) => member.project_id);
            if (edit.op === "reset_overrides") {
              for (const id of edit.project_ids) delete preferenceOverrides[id]?.[row.key];
            } else for (const id of targets) {
              const values = id === null ? preferenceValues : (preferenceOverrides[id] ??= {});
              // The service merges a retention rule into the stored object; it replaces nothing else.
              if (edit.op === "set" && row.key === "storage_cleanup" && typeof edit.value === "object" && edit.value !== null && !Array.isArray(edit.value)) {
                const kept = values[row.key];
                values[row.key] = { ...(kept !== null && typeof kept === "object" && !Array.isArray(kept) ? kept : {}), ...edit.value };
              } else if (edit.op === "set") values[row.key] = edit.value;
              else delete values[row.key];
            }
          }
          return { status: 200, body: scopedPreferences(change.scope, Object.keys(change.edits)) };
        }
        settings = data as ServerSettings;
        return { status: 200, body: settings };
      case "GET /api/workspace":
        return { status: 200, body: workspaceBody() };
      case "POST /api/workspace":
        if (seg[2] === "setup") return setup();
        if (seg[2] === "update") {
          if (managed.library?.dirty) return err(409, "library_dirty", "The library has local changes.");
          if (activeRuns() > 0) return err(409, "runs_active", "Runs are active.");
          const list = updates.library.incoming;
          if (data.apply === true) finishStep("library");
          return {
            status: 200,
            body: {
              applied: data.apply === true,
              previous_revision: managed.library?.previous_revision ?? null,
              revision: managed.library?.revision ?? "",
              incoming: list,
            },
          };
        }
        if (seg[2] === "rollback" && managed.library?.previous_revision) {
          const revision = managed.library.previous_revision;
          managed = { ...managed, library: { ...managed.library, revision, previous_revision: null } };
          return { status: 200, body: { applied: true, previous_revision: null, revision, incoming: [] } };
        }
        break;
      case "DELETE /api/projects": {
        const project = projects.find((entry) => entry.id === seg[2]);
        if (!project) return err(404, "not_found", "Project not found.");
        if (threads.some((thread) => thread.project_id === project.id && thread.active_run)) {
          return err(409, "thread_busy", "One of this project's threads is running.");
        }
        const row = workspaceBody().targets.find((target) => target.project_id === project.id);
        const deleteFiles = query.get("delete_files") === "true";
        if (deleteFiles && !project.managed) return err(400, "invalid_input", "This folder can only be removed from the list.");
        if (deleteFiles && row?.dirty && query.get("confirm") !== "true") {
          return err(409, "project_has_work", "This checkout has uncommitted changes.");
        }
        projects = projects.filter((entry) => entry.id !== project.id);
        threads = threads.filter((thread) => thread.project_id !== project.id);
        return { status: 200, body: { files_removed: deleteFiles, kept_path: deleteFiles ? null : project.path } };
      }
      case "GET /api/browser-profiles":
        return scenario.startsWith("browser-profiles") ? { status: 200, body: browserProfiles } : err(404, "not_found", "Not found.");
      case "POST /api/browser-profiles": {
        // `/api/browser-profiles/<id>/clear` is answered below.
        if (seg[2]) break;
        const name = typeof data.name === "string" ? data.name.trim() : "";
        if (!name) return err(400, "invalid_input", "A profile needs a name.");
        const made = { id: `bp-${++idCounter}`, name, in_use: false };
        browserProfiles = [...browserProfiles, made];
        return { status: 200, body: made };
      }
      case "GET /api/keybindings":
        return { status: 200, body: keybindingsMock.get() };
      case "POST /api/keybindings":
        return keybindingsMock.save(data as unknown as KeybindingRule);
      // No engine host is reported: the real client reads a 404 as "nothing reported".
      case "GET /api/engine-host":
        return err(404, "not_found", "Not found.");
      case "GET /api/info":
        return { status: 200, body: { ...fixtures.info, ...(scenario.startsWith("platform-") ? { platform: scenario.slice("platform-".length) } : {}), ...infoPatch, remote: scenario === "remote", ...(scenario === "crew" || scenario.startsWith("crew-") ? { crew_preview: true } : {}) } };
      case "POST /auth/session":
        if (data.key === "orbyn-demo-key") {
          signedIn = true;
          return { status: 204 };
        }
        return err(401, "invalid_key", "That access key isn't valid.");
      case "GET /api/fs": {
        const given = foldersMock.list(query.get("path"));
        if (given) return { status: 200, body: given };
        const at = query.get("path") ?? "/Users/dev";
        const node = fixtures.fsTree[at];
        if (!node) return { status: 200, body: { path: at, parent: fixtures.fsTree[at.split("/").slice(0, -1).join("/")] ? at.split("/").slice(0, -1).join("/") : "/Users/dev", entries: [] } };
        const join = (name: string) => (at === "/" ? `/${name}` : `${at}/${name}`);
        return {
          status: 200,
          body: {
            path: at,
            parent: node.parent,
            entries: node.entries.map((entry) => ({
              name: entry.name,
              path: join(entry.name),
              is_dir: true,
              is_git: entry.is_git ?? false,
            })),
          },
        };
      }
      case "GET /api/projects": {
        if (seg[3] === "icon") {
          const project = projects.find((entry) => entry.id === seg[2]);
          return project ? { status: 200, body: { project_id: project.id, project_path: project.path, image: null } }
            : err(404, "not_found", "Project not found.");
        }
        return { status: 200, body: projects };
      }
      case "POST /api/projects": {
        if (seg[2] === "clone") {
          const url = String(data.url ?? "");
          if (!/^(https?:\/\/|git@)/.test(url)) return err(400, "invalid_url", "Enter a Git URL.");
          return { status: 201, body: newProject(`/Users/dev/orbyn/${url.split("/").at(-1)}`) };
        }
        if (seg[2] === "create") {
          return { status: 201, body: newProject(`/Users/dev/orbyn/${String(data.name)}`, true) };
        }
        if (seg[2] === "add") {
          return { status: 201, body: newProject(String(data.path ?? "")) };
        }
        break;
      }
      case "DELETE /api/queue":
        queued = queued.filter((item) => item.id !== seg[2]);
        return { status: 204 };
      case "GET /api/editors":
        return { status: 200, body: [{ id: "code", name: "VS Code", launch_style: "goto", file_manager: false }] };
      case "GET /api/pulls":
        if (scenario === "pulls-none") return { status: 200, body: { pulls: [], failed: [] } };
        if (scenario === "pulls-unreachable") return { status: 200, body: { pulls: [], failed: [{ project_id: "orbyn-engineering-app", message: "GitHub did not answer." }] } };
        if (scenario.startsWith("pulls") && query.get("state") === "closed") {
          // A second list with its own numbers, so the details of the first do not cover it.
          const closed = fixtures.pullList().map((entry) => ({ ...entry, pull: { ...entry.pull, number: entry.pull.number + 1000, state: "closed" } }));
          return { status: 200, body: { pulls: closed, failed: [] } };
        }
        if (scenario === "pulls-ready") {
          const ready = fixtures.pullList().map((one) => (one.pull.number === 137 ? { ...one, pull: { ...one.pull, review_state: "approved" } } : one));
          return { status: 200, body: { pulls: ready, failed: [] } };
        }
        return { status: 200, body: { pulls: scenario.startsWith("pulls") ? fixtures.pullList() : fixtures.pulls(), failed: [] } };
      case "POST /api/pulls": {
        if (seg[2] !== "details") break;
        if (scenario === "pulls-details-failed") return err(502, "forge_unreachable", "GitHub did not answer.");
        const asked = ((data.pulls ?? []) as { project_id: string; number: number }[]).map((one) => `${one.project_id}:${one.number}`);
        const all = scenario.startsWith("pulls") ? [...fixtures.pullDetails(), ...fixtures.pullDetails().map((one) => ({ ...one, number: one.number + 1000 }))] : [];
        if ((scenario === "pulls-limited" || scenario === "pulls-limited-still") && detailsReads++ > 0) {
          // Like the service: every detail field is withheld, no row carries one, and the answer says when the pause ends.
          const fields = ["created_at", "requested_reviewers", "viewer_review_state", "comment_count", "approval_count"];
          return {
            status: 200,
            body: {
              details: all
                .filter((one) => asked.includes(`${one.project_id}:${one.number}`))
                .map((one) => Object.fromEntries(Object.entries(one).filter(([name]) => !fields.includes(name)))),
              paused: fields,
              resume_at: new Date(Date.now() + (scenario === "pulls-limited" ? 600_000 : 2_000)).toISOString(),
            },
          };
        }
        return {
          status: 200,
          body: {
            details: all
              .filter((one) => asked.includes(`${one.project_id}:${one.number}`))
              .map((one) => (scenario === "pulls-rate-limited" ? { ...one, comment_count: undefined, approval_count: undefined } : one)),
            paused: scenario === "pulls-rate-limited" ? ["comment_count", "approval_count"] : [],
          },
        };
      }
      case "GET /api/forges":
        return {
          status: 200,
          body: [
            scenario === "pulls-signed-out"
              ? { id: "github", name: "GitHub", program: "gh", installed: true, version: "2.60.0", signed_in: false, accounts: [], problem: null }
              : { id: "github", name: "GitHub", program: "gh", installed: true, version: "2.60.0", signed_in: true, accounts: ["you"], viewer: "you", problem: null },
          ],
        };
      case "GET /api/clones":
        return { status: 200, body: clones };
      case "POST /api/clones": {
        // A bad URL fails as the job starts, which is when the toast has to announce it.
        if (cloneError !== null) {
          const failed = { id: `c-${clones.length}`, url: data.url, name: null, path: null, state: "failed", project: null, error: cloneError, used_https: false, started_at: now() };
          cloneError = null;
          clones = [...clones, failed];
          return { status: 202, body: failed };
        }
        const project = newProject(String(data.url ?? "repo").split("/").at(-1)?.replace(".git", "") ?? "repo");
        const job = { id: `c-${clones.length}`, url: data.url, name: null, state: "done", project, error: null, used_https: false, started_at: now() };
        clones = [...clones, job];
        return { status: 202, body: job };
      }
      case "GET /api/usage":
        return {
          status: 200,
          body: { host_id: "mock", scanned_at: now(), engines: fixtures.usageEngines() },
        };
      case "GET /api/limits": {
                if (!scenario.startsWith("limits")) return { status: 200, body: { pooled: [], engines: [], unavailable: [] } };
        return { status: 200, body: fixtures.limits(scenario, now()) };
      }
      case "PATCH /api/threads": {
        const thread = threads.find((t) => t.id === seg[2]);
        if (!thread) return err(404, "not_found", "Thread not found.");
        const stamp = now();
        const base = thread.lifecycle ?? fixtures.lifecycle();
        const patch = data as UpdateThread;
        const lifecycle = {
          ...base,
          ...(patch.pinned === undefined ? {} : { pinned_at: patch.pinned ? stamp : null }),
          ...(patch.settled === undefined ? {} : { settled_at: patch.settled ? stamp : null }),
          ...(patch.archived === undefined ? {} : { archived_at: patch.archived ? stamp : null }),
          ...(patch.unread === undefined ? {} : { unread: patch.unread }),
          ...(patch.permission_mode ? { permission_mode: patch.permission_mode } : {}),
        };
        threads = threads.map((t) =>
          t.id === thread.id ? { ...t, lifecycle, title: patch.title ?? t.title, ...(patch.effort ? { effort: patch.effort } : {}) } : t,
        );
        return { status: 200, body: threads.find((t) => t.id === thread.id) };
      }
      case "GET /api/threads": {
        if (seg[2]) {
          if (seg[3] === "terminals") return { status: 200, body: [] };
          if (seg[3] === "tools") return { status: 200, body: fixtures.engineTools(threads.find((t) => t.id === seg[2])?.engine ?? "claude") };
          if (seg[3] === "branches") return { status: 200, body: [{ name: "main", current: true, remote: false }] };
          if (seg[3] === "scripts") return { status: 200, body: { path: null, file: null, problem: null, scripts: [] } satisfies ProjectFileView };
          if (seg[3] === "mentions") {
            const needle = (query.get("q") ?? "").toLowerCase();
            return {
              status: 200,
              body: {
                skills: ["architect", "tdd", "unslop"].filter((n) => n.includes(needle)),
                files: ["README.md", "src/main.rs", "src/lib.rs"].filter((f) => f.toLowerCase().includes(needle)),
                threads: [],
              },
            };
          }
          if (seg[3] === "queue") return { status: 200, body: queued.filter((q) => q.thread_id === seg[2]) };
          if (seg[3] === "commands") return { status: 200, body: fixtures.slashCommands };
          // The Files surface: a small checkout tree, and text for any file in it.
          if (seg[3] === "files" && seg[4] === "content") {
            const file = query.get("path") ?? "";
            const saved = savedFiles.get(`${seg[2]}:${file}`);
            // A thread's file is its own: its text and revision name the thread, so the same path in two threads differs.
            const content = saved?.content ?? `// ${file}\n// Mock file contents of ${seg[2]}.\n`;
            return { status: 200, body: { path: file, content, revision: saved?.revision ?? `mock-${seg[2]}-${file}`, size: content.length, binary: false, truncated: false } };
          }
          if (seg[3] === "files" && !seg[4]) {
            const dir = query.get("path") ?? "";
            const tree: Record<string, [string, boolean][]> = {
              "": [["src", true], ["README.md", false], ["Cargo.toml", false]],
              src: [["main.rs", false], ["lib.rs", false]],
            };
            const entries = (tree[dir] ?? []).map(([name, isDir]) => ({
              name,
              path: dir ? `${dir}/${name}` : name,
              is_dir: isDir,
              size: isDir ? 0 : 120,
              ignored: false,
            }));
            return { status: 200, body: entries };
          }
          if (seg[3] === "previews") return { status: 200, body: [] };
          const thread = threads.find((t) => t.id === seg[2]);
          if (!thread) return err(404, "not_found", "Thread not found.");
          if (seg[3] === "diff") return { status: 200, body: fixtures.diffs[thread.id] ?? [] };
          // Mock threads are Git checkouts with no worktree setup.
          if (seg[3] === "jj" || seg[3] === "setup") return { status: 200, body: null };
          return { status: 200, body: thread };
        }
        const id = query.get("project_id");
        const archived = query.get("archived") === "true";
        return {
          status: 200,
          body: threads
            .filter((t) => (!id || t.project_id === id) && (!archived || Boolean(t.lifecycle?.archived_at)))
            .sort((a, b) => b.updated_at.localeCompare(a.updated_at)),
        };
      }
      case "DELETE /api/threads":
        threads = threads.filter((t) => t.id !== seg[2]);
        return { status: 204 };
      case "GET /api/approvals":
        return { status: 200, body: approvals };
      case "GET /api/skills":
        return {
          status: 200,
          body: {
            version: "0.15",
            digest: "0".repeat(40),
            signature_valid: true,
            source: null,
            skills: fixtures.skills,
          },
        };
      case "GET /api/providers": {
        const list = providerList(engines);
        return { status: 200, body: { ...list, providers: [...list.providers, ...foundPrograms] } };
      }
      case "GET /api/engines":
        if (seg[3] === "native-sessions" && seg[4] === "count") {
          return { status: 200, body: { engine: seg[2], ...(nativeCounts.get(seg[2] ?? "") ?? { count: fixtures.nativeSessionCounts[seg[2] ?? ""] ?? 0, truncated: false }) } };
        }
        // An ACP agent's own saved conversations: the mock agent lists one and says it can load and delete it.
        if (seg[3] === "native-sessions" && seg[4] === "capabilities") return { status: 200, body: { can_list: true, can_load: true, can_resume: false, can_delete: true } };
        if (seg[3] === "native-sessions" && !seg[4]) {
          const chat = { session_id: "native-1", cwd: projects[0]?.path ?? "/Users/dev/code", additional_directories: [], title: "First chat", updated_at: now(), imported_thread_id: importedNative.get(`${seg[2]}:native-1`) ?? null };
          return { status: 200, body: { sessions: nativeDeleted.has(`${seg[2]}:native-1`) ? [] : [chat], next_cursor: null, can_load: true, can_resume: false, can_delete: true } };
        }
        if (seg[3] === "installs") {
          // Only an engine that is not installed has the vendor's page.
          const missing = engines.find((e) => e.id === seg[2])?.installed === false;
          return { status: 200, body: { engine: seg[2], ...(missing ? { vendor_page: { url: `https://${seg[2]}.example/install`, domain: `${seg[2]}.example` } } : {}), outcomes: installs[seg[2] ?? ""] ?? [] } };
        }
        if (seg[3] === "limits") return err(502, "engine_unreachable", "The engine did not answer.");
        if (seg[3] === "tools") return { status: 200, body: fixtures.engineTools(seg[2] ?? "claude") };
        if (seg[3] === "models") {
          // The service merges the person's custom models into the list it returns.
          const listing = modelLists[seg[2] ?? ""] ?? fixtures.engineModels(seg[2] ?? "");
          const listed: ModelInfo[] = listing.models;
          const custom = (Array.isArray(preferenceValues.custom_models) ? preferenceValues.custom_models : [])
            .flatMap((entry) => entry && typeof entry === "object" && !Array.isArray(entry) && entry.engine === seg[2] && typeof entry.id === "string"
              ? [{ id: entry.id, label: typeof entry.label === "string" ? entry.label : entry.id, description: null, is_default: false, custom: true, options: [] }] : []);
          return { status: 200, body: { ...listing, models: [...listed, ...custom] } };
        }
        return { status: 200, body: engines };
      default:
        break;
    }

    if (method === "POST" && seg[0] === "api") {
      if (seg[1] === "browser-profiles" && seg[3] === "clear") return { status: 204 };
      if (seg[1] === "projects" && seg[3] === "trust") {
        const project = projects.find((p) => p.id === seg[2]);
        if (!project) return err(404, "not_found", "Project not found.");
        project.trusted = true;
        return { status: 200, body: project };
      }
      // A path a message names: inside the thread's checkout it is that file; a full path outside is not.
      if (seg[1] === "threads" && seg[3] === "paths" && seg[4] === "resolve") {
        const thread = threads.find((t) => t.id === seg[2]);
        if (!thread) return err(404, "not_found", "Thread not found.");
        const path = String(data.path ?? "");
        const inside = !path.startsWith("/") || path.startsWith(`${thread.cwd}/`);
        return { status: 200, body: { cwd: thread.cwd, checkout_path: inside ? path.replace(`${thread.cwd}/`, "") : null, repository_path: null } };
      }
      if (seg[1] === "threads" && !seg[2]) {
        const engine = engines.find((e) => e.id === data.engine);
        const project = projects.find((p) => p.id === data.project_id);
        if (!engine || !project) return err(400, "invalid_request", "Unknown engine or project.");
        const purpose = typeof data.worktree === "string" && data.worktree ? data.worktree : null;
        const thread: Thread = {
          scratch: false,
          id: nextId("t"),
          project_id: project.id,
          title: typeof data.title === "string" && data.title ? data.title : "New thread",
          engine: engine.id,
          model: typeof data.model === "string" ? data.model : null,
          cwd: purpose ? `${project.path}-wt/${purpose}` : project.path,
          worktree: purpose
            ? { path: `${project.path}-wt/${purpose}`, branch: purpose, base: "main" }
            : null,
          skill_profile: typeof data.skill_profile === "string" ? data.skill_profile : null,
          active_run: null,
          last_run_state: null,
          created_at: new Date(fixtures.T0 + 3600_000).toISOString(),
          updated_at: new Date(fixtures.T0 + 3600_000).toISOString(),
        };
        threads = [...threads, thread];
        return { status: 201, body: thread };
      }
      if (seg[1] === "threads" && seg[3] === "queue" && !seg[4]) {
        const item: QueuedMessage = {
          id: nextId("q"),
          thread_id: seg[2] ?? "",
          prompt: String(data.prompt),
          attachment_ids: [],
          held: false,
          created_at: now(),
        };
        queued = [...queued, item];
        return { status: 201, body: item };
      }
      if (seg[1] === "threads" && seg[3] === "steer") return { status: 202 };
      if (seg[1] === "threads" && seg[3] === "fork") {
        const source = threads.find((t) => t.id === seg[2]);
        if (!source) return err(404, "not_found", "Thread not found.");
        const side = data.side === true;
        const fork: Thread = {
          ...source,
          id: nextId("t"),
          title: side ? `Side chat: ${source.title}` : `${source.title} (fork)`,
          active_run: null,
          last_run_state: null,
          created_at: now(),
          updated_at: now(),
          lifecycle: source.lifecycle
            ? { ...source.lifecycle, parent_thread_id: source.id, relation: side ? "side" : null, pinned_at: null, settled_at: null }
            : source.lifecycle,
        };
        threads = [...threads, fork];
        return { status: 201, body: fork };
      }
      if (seg[1] === "queue" && seg[3] === "send-now") {
        queued = queued.filter((item) => item.id !== seg[2]);
        return { status: 202 };
      }
      if (seg[1] === "threads" && seg[3] === "attachments") {
        const name = query.get("name") ?? "file";
        const snap = /snap shot/i.test(name)
          ? { source: { kind: "snap_shot", captured_at: now(), app_name: "Safari", window_title: "Acme · Sign in", app_id: "com.apple.Safari", width: 1280, height: 800 } }
          : {};
        return { status: 201, body: { id: nextId("att"), thread_id: seg[2], name, mime: "", size: 1, created_at: now(), ...snap } };
      }
      if (seg[1] === "threads" && seg[3] === "runs") {
        const thread = threads.find((t) => t.id === seg[2]);
        if (!thread) return err(404, "not_found", "Thread not found.");
        return startRun(thread, String(data.prompt ?? ""));
      }
      if (seg[1] === "runs" && seg[3] === "cancel") {
        const thread = threads.find((t) => t.active_run?.id === seg[2]);
        if (!thread) return err(404, "not_found", "No active run.");
        const runId = seg[2] ?? "";
        for (const approval of approvals.filter((a) => a.run_id === runId)) {
          append(thread.id, runId, fixtures.approvalEvent({ ...approval, state: "expired" }));
        }
        approvals = approvals.filter((a) => a.run_id !== runId);
        append(thread.id, runId, {
          type: "RUN_FINISHED",
          threadId: thread.id,
          runId,
          outcome: { type: "cancelled" },
        });
        setRun(thread.id, { active_run: null, last_run_state: "cancelled" });
        return { status: 204 };
      }
      if (seg[1] === "approvals" && seg[3] === "decide") {
        const approval = approvals.find((a) => a.id === seg[2]);
        if (!approval) return err(409, "already_decided", "This approval was already decided.");
        return decide(approval, data.allow === true);
      }
      if (seg[1] === "engines" && seg[2] === "refresh") return { status: 200, body: engines };
      if (seg[1] === "engines" && seg[3] === "native-sessions" && seg[4] === "import") {
        // The thread the answer names exists afterwards, in the project whose folder the chat was in.
        const project = projects[0];
        if (!project) return err(404, "not_found", "No project to import into.");
        const thread_id = nextId("t");
        importedNative.set(`${seg[2]}:${String(data.session_id)}`, thread_id);
        threads = [...threads, {
          scratch: false, id: thread_id, project_id: project.id, title: "First chat", engine: seg[2] ?? "", model: null, cwd: project.path,
          worktree: null, skill_profile: null, active_run: null, last_run_state: null, created_at: now(), updated_at: now(),
        }];
        return { status: 200, body: { thread_id, imported: true } };
      }
      if (seg[1] === "engines" && seg[3] === "native-sessions" && seg[4] === "delete") {
        nativeDeleted.add(`${seg[2]}:${String(data.session_id)}`);
        return { status: 204 };
      }
      if (seg[1] === "engines" && seg[3] === "install-recommended") {
        const engine = engines.find((e) => e.id === seg[2]);
        if (!engine) return err(404, "not_found", "Engine not found.");
        if (!engine.compatibility?.recommended_command) return err(422, "update_unavailable", `No recommended version is recorded for ${engine.id}.`);
        engine.version = engine.compatibility.recommended_version ?? engine.version;
        engine.support = "tested";
        engine.compatibility = undefined;
        return { status: 200, body: null };
      }
      // Codex reports a problem to OpenAI and answers with the thread it opened; no other engine has the route.
      if (seg[1] === "engines" && seg[3] === "feedback") {
        if (seg[2] !== "codex") return err(422, "unsupported", "This engine cannot send feedback.");
        return { status: 200, body: { thread_id: nextId("report"), prompt_hash: null } };
      }
      // Updates every engine that owns an installer and has a newer version, one after another, as one update each would.
      if (seg[1] === "engines" && seg[2] === "update-all") {
        const outcomes = engines.filter((engine) => engine.installed && !engine.updating && !engine.instance_of && engine.update_command !== null && engine.latest !== null && engine.latest !== engine.version).map((engine) => {
          const answer = takeFailure(`/api/engines/${engine.id}/update`, "POST") ?? route("POST", `/api/engines/${engine.id}/update`, new URLSearchParams(), {});
          return answer.status < 400 ? { id: engine.id, name: engine.name, version: engine.latest ?? undefined } : { id: engine.id, name: engine.name, error: String((answer.body as { message?: string }).message) };
        });
        return { status: 200, body: outcomes };
      }
      if (seg[1] === "engines" && seg[2] === "probe-binary") {
        // As probe.rs answers: a relative path is refused with a sentence, a missing file is just not found, and a program whose version command fails is found with a problem.
        const path = String(data.path ?? "");
        if (!path.startsWith("/")) return { status: 200, body: { exists: false, path: null, version: null, problem: "Give the full path to the program." } };
        if (path.endsWith("/claude")) return { status: 200, body: { exists: true, path, version: "2.1.318", problem: null } };
        if (path.endsWith("/broken")) return { status: 200, body: { exists: true, path, version: null, problem: "Its version command failed." } };
        return { status: 200, body: { exists: false, path: null, version: null, problem: null } };
      }
      if (seg[1] === "shell" && seg[2] === "actions") return { status: 202 };
      if (seg[1] === "engines" && seg[2] === "custom") {
        const tier = data.tier as EngineStatus["tier"];
        const engine: EngineStatus = {
          id: String(data.id),
          name: String(data.name),
          tier,
          capabilities: {
            streaming: tier !== "terminal",
            goal_control: false,
            decline_feedback: false,
            permission_modes: ["supervised", "auto_edits", "auto", "full_access"],
            approvals: tier !== "terminal",
            resume: false,
            mcp: false,
            sandbox: false,
            managed: tier !== "terminal",
            skill_delivery: [],
          },
          installed: true,
          path: String(data.target),
          version: null,
          auth: "not_applicable",
          installer: "unknown",
          latest: null,
          support: "unknown",
          update_command: null,
          manual_hint: "Custom engines are updated by you.",
          sign_in_command: null,
          updating: false,
          problem: null,
        };
        engines = [...engines, engine];
        return { status: 201, body: engine };
      }
      if (seg[1] === "engines" && seg[3] === "update") {
        const engine = engines.find((e) => e.id === seg[2]);
        if (!engine) return err(404, "not_found", "Engine not found.");
        if (threads.some((t) => t.engine === engine.id && t.active_run)) {
          return err(409, "engine_busy", `${engine.name} has an active run. Finish or cancel it first.`);
        }
        engine.updating = true;
        const started = now();
        const from = engine.version;
        const command = engine.update_command ?? "";
        const timer = setTimeout(() => {
          engine.updating = false;
          finishStep(`engine:${engine.id}`);
          engine.version = engine.latest;
          keepInstall({
            id: `${engine.id}-${started}`, engine: engine.id, kind: "update", started_at: started, finished_at: now(), ok: true,
            command, exit_status: 0, from_version: from ?? undefined, to_version: engine.latest ?? undefined, output: ["added 1 package"],
          });
        }, stepMs * 20);
        timers.add(timer);
        return { status: 202 };
      }
    }

    if (method === "DELETE" && seg[0] === "api") {
      if (seg[1] === "browser-profiles" && seg[2]) {
        browserProfiles = browserProfiles.filter((entry) => entry.id !== seg[2]);
        return { status: 204 };
      }
      if (seg[1] === "acp" && seg[2] === "registry") return { status: 204 };
      if (seg[1] === "engines" && seg[2] === "custom") {
        if (threads.some((t) => t.engine === seg[3] && t.active_run)) {
          return err(409, "engine_busy", `${seg[3]} has an active run or update`);
        }
        engines = engines.filter((e) => e.id !== seg[3]);
        const at = now();
        keepInstall({ id: `${seg[3]}-${at}`, engine: seg[3] ?? "", kind: "remove", started_at: at, finished_at: at, ok: true, output: [] });
        return { status: 204 };
      }
    }
    return err(404, "not_found", `No mock for ${method} ${path}.`);
  }

  return {
    reset,
    get scenario() {
      return scenario;
    },
    get delayMs() {
      return delayMs;
    },
    set delayMs(value: number) {
      delayMs = value;
    },
    get threads() {
      return threads;
    },
    /** Queues a failure for the next `times` requests under `path` (only those with this `method`, when one is given). */
    fail(path: string, times: number, status = 500, code = "internal", message = "Something went wrong.", method?: string) {
      failures.push({ path, remaining: times, status, code, message, method });
    },
    handle(method: string, url: string, body?: unknown, confirm?: string): MockResponse {
      const parsed = new URL(url, "http://mock.local");
      if (scenario === "offline" && parsed.pathname.startsWith("/api")) {
        return err(503, "unavailable", "The service is unavailable.");
      }
      const refused = takeFailure(parsed.pathname, method);
      if (refused) return refused;
      if (parsed.pathname.startsWith("/api/code-runs") || /^\/api\/engines\/[^/]+\/code-mode$/.test(parsed.pathname) || /^\/api\/threads\/[^/]+\/code-runs$/.test(parsed.pathname)) {
        const answered = codeModeMock.handle(method, parsed.pathname, body);
        if (answered) return answered;
      }
      if (parsed.pathname.startsWith("/api/account")) return accountMock.handle(method, parsed.pathname, body);
      if (parsed.pathname.startsWith("/api/settings/secrets")) return secretsMock.handle(method, parsed.pathname, body);
      if (parsed.pathname.startsWith("/api/crew")) return reviewControlsMock.handle(scenario, method, parsed.pathname, body) ?? mapMock(scenario, method, parsed.pathname, parsed.searchParams) ?? boardMock(scenario, method, parsed.pathname, parsed.searchParams) ?? flowMock(scenario, method, parsed.pathname, parsed.searchParams) ?? backgroundMock.handle(method, parsed.pathname, parsed.searchParams, body) ?? needsYouMock(scenario, method, parsed.pathname) ?? escapesMock.handle(method, parsed.pathname, parsed.searchParams, body) ?? lessonsMock.handle(method, parsed.pathname, parsed.searchParams, body) ?? proposalsMock.handle(method, parsed.pathname, parsed.searchParams, body) ?? backtestMock.handle(method, parsed.pathname) ?? capacityMock.handle(scenario, method, parsed.pathname, body) ?? crewMock.handle(method, parsed.pathname, body, parsed.searchParams, confirm);
      return route(method, parsed.pathname, parsed.searchParams, body);
    },
    /** The thread terminals and their command blocks: seed what the service lists, read it back after a write. */
    terminals: terminalsMock,
    /** The engine's questions: put one in front of a thread, read how it was settled. */
    questions: questionsMock,
    /** The folders the browser lists, in place of the sample tree: `folders.set(home, { "/Users/me": ["Documents"] })`. */
    folders: foldersMock,
    /** The Code Mode stand-in (mock/codemode.ts): seed a run record, read the writes, switch the routes off (404). */
    codeMode: codeModeMock,
    /** Sets what the next dry run of Delete now finds. */
    setCleanupPreview(preview: CleanupPreview) {
      cleanupPreviewOverride = preview;
    },
    /** Seeds the Keybindings page with a view it cannot reach by clicking (a clash, a rule with a context). */
    setKeybindings(view: KeybindingsView) {
      keybindingsMock.set(view);
    },
    /** Changes the account has waiting to upload, for the offline and paused wording. */
    setAccountWaiting(count: number) {
      accountMock.setWaiting(count);
    },
    /** Account knobs for the states a spec cannot reach by clicking; see `createAccountMock().setKnobs`. */
    setAccount(knobs: Parameters<typeof accountMock.setKnobs>[0]) {
      accountMock.setKnobs(knobs);
    },
    events(threadId: string, after: number): WireThreadEvent[] {
      return (logs[threadId] ?? []).filter((event) => event.seq > after);
    },
    subscribe(threadId: string, subscriber: Subscriber): () => void {
      const set = subscribers.get(threadId) ?? new Set<Subscriber>();
      subscribers.set(threadId, set);
      set.add(subscriber);
      return () => set.delete(subscriber);
    },
    /** Makes the next setup attempt end in the failed state. */
    failSetup() {
      failNextSetup = true;
    },
    /** Makes the next clone end failed, with the Git error a bad URL answers. */
    failClone(message = "fatal: repository 'bad url with spaces' does not exist") {
      cloneError = message;
    },
    /** Fails the given step of the next apply, once. */
    failUpdateStep(id: string) {
      failStep = id;
    },
    /** Changes what `/api/info` answers from now on, such as the build `channel` or `version`. */
    patchInfo(patch: Partial<ServerInfo>) {
      infoPatch = { ...infoPatch, ...patch };
    },
    /** Gives an engine this model list (the service merges the person's custom models into it). */
    setEngineModels(engine: string, models: EngineModels | ModelInfo[]) {
      // A bare list keeps the engine's own default and options; a full listing replaces them.
      const listing: EngineModels = Array.isArray(models) ? { ...fixtures.engineModels(engine), models: structuredClone(models) } : models;
      modelLists = { ...modelLists, [engine]: listing };
    },
    patchUpdates(patch: Partial<UpdateStatus>) {
      updates = { ...updates, ...patch };
    },
    /** Replaces what Settings › Browser lists (the "browser-profiles" scenario answers it), for ids and names the scenario does not hold. */
    setBrowserProfiles(rows: { id: string; name: string; in_use: boolean }[]) {
      browserProfiles = rows;
    },
    /** Changes what Settings › Maintenance reads as the service's diagnostics, until the next reset. */
    patchDiagnostics(patch: Partial<Diagnostics>) {
      diagnosticsPatch = { ...diagnosticsPatch, ...patch };
    },
    /** Replaces the extra programs the provider list reports (rows for what is on the computer but is not an engine). */
    setFoundPrograms(entries: ProviderEntry[]) {
      foundPrograms = entries;
    },
    /** Sets how many saved conversations an engine keeps; `truncated` says the folder was too large to count to the end. */
    setNativeSessionCount(engine: string, count: number, truncated = false) {
      nativeCounts.set(engine, { count, truncated });
    },
    /** Settings treats these projects as checkouts of one repository: one group, selected as `mock:group:<ids joined by +>`. */
    groupProjects(ids: string[]) {
      groupedProjects = ids;
    },
    /** Replaces the engines the service reports, for states the fixtures do not hold (an engine below its supported range, one that cannot be updated). */
    setEngines(next: EngineStatus[]) {
      engines = structuredClone(next);
    },
    patchWorkspace(patch: Partial<WorkspaceStatus>) {
      managed = { ...managed, ...patch };
    },
    get workspace() {
      return managed;
    },
    get updates() {
      return updates;
    },
  };
}

export type MockApi = ReturnType<typeof createMockApi>;
