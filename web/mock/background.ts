/**
 * The mock's background crews (Crew designs: background-list and the rest): the four crews the designs draw, their runs, the
 * watch crews of KC6 with what their last check found, and the writes the Background screen makes (create, on and off). It
 * answers only in the `crew-background*` scenarios, and the state is rebuilt on every reset, so a test never sees another's
 * crews. A `crew-background-watch-*` scenario is one watch's page: the same crew list with that watch's last check and its
 * changes, so `#/crew/background` plus a click on its row draws the state the frame draws. The other Crew scenarios keep the
 * read-only fixture of `mock/needsyou.ts`.
 */
import type { BackgroundCrew } from "../src/generated/BackgroundCrew.ts";
import type { BackgroundCrewCreate } from "../src/generated/BackgroundCrewCreate.ts";
import type { BackgroundCrewUpdate } from "../src/generated/BackgroundCrewUpdate.ts";
import type { BackgroundRun } from "../src/generated/BackgroundRun.ts";
import type { BackgroundWatchChange } from "../src/generated/BackgroundWatchChange.ts";
import type { BackgroundWatchStatus } from "../src/generated/BackgroundWatchStatus.ts";
import type { MockResponse } from "./api.ts";

const budget = { turns: 40, minutes: 60 };
const daily = { runs: 2, turns: 80 };
const crew = (id: string, name: string, template: BackgroundCrew["template"], trigger: BackgroundCrew["trigger"], enabled: boolean, state: BackgroundCrew["state"], reviews: number): BackgroundCrew => ({
  id, project_id: "p-app", name, template, trigger, budget, daily, enabled, state, reviews_waiting: reviews, created_at: "2026-11-01T10:00:00Z", updated_at: "2026-11-07T14:00:00Z",
});
const run = (id: string, crewId: string, state: BackgroundRun["state"], started: string, ended: string | null, extra: Partial<BackgroundRun> = {}): BackgroundRun => ({
  id, crew_id: crewId, state, turns_spent: 12, minutes_spent: 6, budget, review_waiting: state === "needs_review", started_at: started, updated_at: ended ?? started, ended_at: ended, ...extra,
});

/** A watch crew: the trigger carries its command, its schedule and what a failing result does. */
const watch = (id: string, name: string, command: string, every: number, onFailing: "needs_you" | "template", enabled: boolean, template: BackgroundCrew["template"] = "fix_failing_ci"): BackgroundCrew =>
  crew(id, name, template, { kind: "watch", command, every_hours: every, on_failing: onFailing }, enabled, "idle", 0);

/** A watch's last check; the unread fields stay absent as the contract's optional ones do. */
const status = (crew_id: string, state: BackgroundWatchStatus["state"], over: Partial<BackgroundWatchStatus> = {}): BackgroundWatchStatus => ({
  crew_id, state, baseline_done: true, error_streak: 0, ...over,
});
/** A refused template start's fixed sentence, code by code, as core's `refusal_sentence` words it (docs/crew.md). */
const REFUSAL_SENTENCE: Record<string, string> = {
  disk_low: "Not started: not enough free space. Free space and try again.",
  no_engine: "No engine is set up for this template. Choose one in Settings.",
  folder_untrusted: "This folder is not trusted yet. Trust it in Settings.",
  run_in_progress: "A run of this crew is already running.",
  not_allowed: "Your settings do not allow this start.",
  crew_gone: "This watch no longer exists.",
  refused: "The start was refused. Open the run list for details.",
};
/** A change row. A refused start names its code, and the read turns it into the fixed sentence core owns; every other row has neither. */
const change = (id: number, kind: BackgroundWatchChange["kind"], action: BackgroundWatchChange["action"], at: string, exit_code: number | null, tail: string | null = null, reasonCode: string | null = null): BackgroundWatchChange => {
  const row = { id, kind, action, at, exit_code, tail };
  const reason = reasonCode ? REFUSAL_SENTENCE[reasonCode] : undefined;
  return reason ? { ...row, reason, reason_code: reasonCode } : row;
};

const T = "2026-11-07T";
const PASSING_TAIL = "> npm test web/nav/redirect\nTests: 12 passed, 12 total";
/** The tail a failing check kept: the output watch-failing draws, and the change detail watch-history draws. */
const FAILING_TAIL = [
  "> npm test web/nav/redirect",
  "",
  "FAIL web/nav/redirect.test.ts",
  "  ● redirect › keeps next when it points inside the app",
  "    expected \"/billing\", received \"/home\"",
  "      Tests: 1 failed, 11 passed, 12 total",
].join("\n");
const STILL_FAILING_TAIL = [
  "> npm test web/nav/redirect",
  "",
  "Error: Cannot find module \"./redirect.helpers\"",
  "  at Object.<anonymous> (web/nav/redirect.test.ts:3:1)",
  "  Test suite failed to run",
].join("\n");

const baseline = change(1, "baseline", "none", "2026-11-05T09:02:00Z", 0);
// The change that turned the watch failing kept the output too: watch-history's change detail reads it from this row.
const raisedAt1402 = change(4, "failing", "needs_you", T + "14:02:00Z", 1, FAILING_TAIL);
const recoveredYesterday = change(3, "passing", "resolved", "2026-11-06T09:02:00Z", 0);
const raisedThursday = change(2, "failing", "needs_you", "2026-11-05T16:02:00Z", 1);

/** The list the watch-list frame draws: four watches in their states beside a normal crew. */
const WATCH_LIST: BackgroundCrew[] = [
  watch("bg-watch-docs", "Docs links", "npm run docs:links", 24, "needs_you", false),
  crew("bg-ci", "Fix failing CI", "fix_failing_ci", { kind: "ci_failure", workflow: "ci.yml", branch: "main" }, true, "needs_review", 1),
  watch("bg-watch-lock", "Lockfile check", "npm run lockfile:check", 6, "needs_you", true),
  watch("bg-watch-main", "Main build", "npm run build", 1, "template", true),
  watch("bg-watch-redirect", "Redirect tests", "npm test web/nav/redirect", 1, "needs_you", true),
];
const WATCH_LIST_WATCHES: Record<string, { status: BackgroundWatchStatus; changes: BackgroundWatchChange[] }> = {
  "bg-watch-docs": {
    status: status("bg-watch-docs", "failing", { last_checked_at: "2026-11-06T15:04:00Z", last_exit_code: 1 }),
    changes: [change(6, "turned_off", "none", T + "09:30:00Z", null), change(5, "failing", "needs_you", "2026-11-06T15:04:00Z", 1), baseline],
  },
  "bg-watch-lock": {
    status: status("bg-watch-lock", "could_not_run", { last_checked_at: T + "12:02:00Z", reason: "timed out", error_streak: 3 }),
    changes: [change(7, "baseline", "none", "2026-11-05T09:10:00Z", 0)],
  },
  "bg-watch-main": {
    status: status("bg-watch-main", "passing", { last_checked_at: T + "16:02:00Z", last_exit_code: 0, tail: PASSING_TAIL }),
    changes: [baseline],
  },
  "bg-watch-redirect": {
    status: status("bg-watch-redirect", "failing", { last_checked_at: T + "16:02:00Z", last_exit_code: 2, tail: STILL_FAILING_TAIL }),
    changes: [raisedAt1402, recoveredYesterday, raisedThursday, baseline],
  },
};

/** One scenario's crews, runs and watch reads. */
interface Seed {
  crews: BackgroundCrew[];
  runs: BackgroundRun[];
  watches: Record<string, { status: BackgroundWatchStatus; changes: BackgroundWatchChange[] }>;
}

/** A watch whose first check failed: Failing from the baseline, and no Needs you row was ever raised (watch-failing's baseline case). */
function failingBaselineScenario(): Seed {
  return {
    crews: [watch("bg-watch-baseline", "Redirect tests", "npm test web/nav/redirect", 1, "needs_you", true)],
    runs: [],
    watches: {
      "bg-watch-baseline": {
        status: status("bg-watch-baseline", "failing", { last_checked_at: "2026-11-05T09:02:00Z", last_exit_code: 1, tail: FAILING_TAIL }),
        changes: [change(1, "baseline", "none", "2026-11-05T09:02:00Z", 1)],
      },
    },
  };
}

/** The seed of one `crew-background*` scenario. */
function seedFor(scenario: string): Seed {
  const empty: Seed = { crews: [], runs: [], watches: {} };
  const base = (): { crews: BackgroundCrew[]; runs: BackgroundRun[] } => ({
    crews: [
      crew("bg-ci", "Fix failing CI", "fix_failing_ci", { kind: "ci_failure", workflow: "ci.yml", branch: "main" }, true, "needs_review", 1),
      crew("bg-flaky", "Flaky test finder", "flaky_test", { kind: "schedule", every_hours: 24 }, true, "idle", 0),
      crew("bg-dep", "Dependency bump", "dependency_bump", { kind: "dependency_update" }, true, "running", 1),
      crew("bg-sec", "Security advisories", "security_advisory", { kind: "dependency_update" }, false, "idle", 0),
    ],
    runs: [
      run("r-ci-1", "bg-ci", "needs_review", "2026-11-07T15:11:00Z", "2026-11-07T15:49:00Z", { turns_spent: 12, minutes_spent: 9 }),
      run("r-ci-0", "bg-ci", "stopped", "2026-11-06T09:20:00Z", "2026-11-06T09:51:00Z", { stop_reason: "budget", turns_spent: 40, minutes_spent: 31, review_waiting: true }),
      run("r-flaky-1", "bg-flaky", "needs_review", "2026-11-06T07:00:00Z", "2026-11-06T07:14:00Z", { turns_spent: 21, minutes_spent: 14 }),
      run("r-dep-2", "bg-dep", "running", "2026-11-07T09:30:00Z", null, { turns_spent: 3, minutes_spent: 6 }),
      run("r-dep-1", "bg-dep", "needs_review", "2026-11-06T09:30:00Z", "2026-11-06T09:34:00Z", { turns_spent: 7, minutes_spent: 4 }),
    ],
  });
  if (scenario === "crew-background-empty") return empty;
  if (scenario === "crew-background") return { ...base(), watches: {} };
  // KC4 PR B (frame kc4-low-disk-background-run): a start the disk guard refused at 16:20 keeps one stopped run, newest first, with the
  // code, the service's sentence, no stop reason, no spend and nobody waiting. "crew-background-reason-unknown" carries a code this
  // page does not know: it must draw as the stopped run it is.
  if (scenario === "crew-background-disk-low" || scenario === "crew-background-disk-low-bare" || scenario === "crew-background-reason-unknown" || scenario === "crew-background-reason-inherited") {
    const seed = base();
    const refused = scenario === "crew-background-disk-low" || scenario === "crew-background-disk-low-bare";
    const bare = scenario === "crew-background-disk-low-bare";
    seed.runs.unshift(run("r-ci-2", "bg-ci", "stopped", "2026-11-07T16:20:00Z", "2026-11-07T16:20:00Z", {
      reason_code: refused ? "disk_low" : scenario === "crew-background-reason-inherited" ? "toString" : "future_code",
      review_reason: bare ? null : refused ? "Not started: 3.1 GB free, Crew needs 5 GB. Free space and try again." : "Stopped for a reason this page does not know.",
      turns_spent: 0, minutes_spent: 0, review_waiting: false,
    }));
    return { ...seed, watches: {} };
  }
  if (scenario === "crew-background-watch") return { crews: WATCH_LIST, runs: [run("r-ci-1", "bg-ci", "needs_review", "2026-11-07T15:11:00Z", "2026-11-07T15:49:00Z", { turns_spent: 12, minutes_spent: 9 })], watches: WATCH_LIST_WATCHES };
  if (scenario === "crew-background-watch-passing") {
    return {
      crews: [watch("bg-watch-redirect", "Redirect tests", "npm test web/nav/redirect", 1, "needs_you", true)],
      runs: [],
      watches: {
        "bg-watch-redirect": {
          status: status("bg-watch-redirect", "passing", { last_checked_at: "2026-11-06T10:02:00Z", last_exit_code: 0, tail: PASSING_TAIL }),
          changes: [recoveredYesterday, raisedThursday, baseline],
        },
      },
    };
  }
  if (scenario === "crew-background-watch-failing") {
    return {
      crews: [watch("bg-watch-redirect", "Redirect tests", "npm test web/nav/redirect", 1, "needs_you", true)],
      runs: [],
      watches: {
        "bg-watch-redirect": {
          status: status("bg-watch-redirect", "failing", { last_checked_at: T + "14:02:00Z", last_exit_code: 1, tail: FAILING_TAIL }),
          changes: [raisedAt1402, recoveredYesterday, raisedThursday, baseline],
        },
      },
    };
  }
  if (scenario === "crew-background-watch-still-failing") {
    return {
      crews: [watch("bg-watch-redirect", "Redirect tests", "npm test web/nav/redirect", 1, "needs_you", true)],
      runs: [],
      watches: {
        "bg-watch-redirect": {
          status: status("bg-watch-redirect", "failing", { last_checked_at: T + "16:02:00Z", last_exit_code: 2, tail: STILL_FAILING_TAIL }),
          changes: [raisedAt1402, recoveredYesterday, raisedThursday, baseline],
        },
      },
    };
  }
  if (scenario === "crew-background-watch-recovered") {
    return {
      crews: [watch("bg-watch-redirect", "Redirect tests", "npm test web/nav/redirect", 1, "needs_you", true)],
      runs: [],
      watches: {
        "bg-watch-redirect": {
          status: status("bg-watch-redirect", "passing", { last_checked_at: T + "17:02:00Z", last_exit_code: 0, tail: PASSING_TAIL }),
          changes: [change(5, "passing", "resolved", T + "17:02:00Z", 0), raisedAt1402, recoveredYesterday, raisedThursday, baseline],
        },
      },
    };
  }
  if (scenario === "crew-background-watch-paused") {
    return {
      crews: [watch("bg-watch-docs", "Docs links", "npm run docs:links", 24, "needs_you", false)],
      runs: [],
      watches: {
        "bg-watch-docs": {
          status: status("bg-watch-docs", "failing", { last_checked_at: "2026-11-06T15:04:00Z", last_exit_code: 1 }),
          changes: [change(3, "turned_off", "none", T + "09:30:00Z", null), change(2, "failing", "needs_you", "2026-11-06T15:04:00Z", 1), baseline],
        },
      },
    };
  }
  if (scenario === "crew-background-watch-failing-baseline") return failingBaselineScenario();
  if (scenario === "crew-background-watch-could-not-run") {
    return {
      crews: [watch("bg-watch-lock", "Lockfile check", "npm run lockfile:check", 6, "needs_you", true)],
      runs: [],
      watches: {
        "bg-watch-lock": {
          status: status("bg-watch-lock", "could_not_run", { last_checked_at: T + "12:02:00Z", reason: "timed out", error_streak: 3 }),
          changes: [change(2, "baseline", "none", "2026-11-05T09:10:00Z", 0)],
        },
      },
    };
  }
  // The frame background-run-detail: one watch whose newest run stopped at its budget, reached by "Open the run".
  if (scenario === "crew-background-run-detail") {
    return {
      crews: [watch("bg-7f3a21", "Main build", "npm run build", 1, "template", true)],
      runs: [run("r-main-1", "bg-7f3a21", "stopped", T + "14:02:00Z", T + "14:48:00Z", { stop_reason: "budget", turns_spent: 40, minutes_spent: 31, review_waiting: true })],
      watches: {
        "bg-7f3a21": {
          status: status("bg-7f3a21", "failing", { last_checked_at: T + "14:02:00Z", last_exit_code: 1 }),
          changes: [change(4, "failing", "started_template", T + "14:02:00Z", 1), baseline],
        },
      },
    };
  }
  // The frame watch-could-not-start-reasons: the seven fixed sentences a refused template start keeps, newest first.
  if (scenario === "crew-background-watch-refused") {
    const refusals: [number, string, string][] = [
      [8, "disk_low", T + "14:02:00Z"],
      [7, "no_engine", T + "13:02:00Z"],
      [6, "folder_untrusted", T + "12:02:00Z"],
      [5, "run_in_progress", T + "11:02:00Z"],
      [4, "not_allowed", T + "10:02:00Z"],
      [3, "crew_gone", T + "09:02:00Z"],
      [2, "refused", "2026-11-06T14:02:00Z"],
    ];
    return {
      crews: [watch("bg-watch-main", "Main build", "npm run build", 1, "template", true)],
      runs: [],
      watches: {
        "bg-watch-main": {
          status: status("bg-watch-main", "failing", { last_checked_at: T + "14:02:00Z", last_exit_code: 1 }),
          changes: refusals.map(([id, code, at]) => change(id, "failing", "needs_you", at, 1, null, code)),
        },
      },
    };
  }
  // The same watch whose newest change started a template run, but its runs read holds none: no "Open the run".
  if (scenario === "crew-background-watch-template-no-run") return { ...seedFor("crew-background-watch-template"), runs: [] };
  if (scenario === "crew-background-watch-template") {
    return {
      crews: [watch("bg-watch-main", "Main build", "npm run build", 1, "template", true)],
      runs: [run("r-main-1", "bg-watch-main", "running", T + "14:02:00Z", null, { turns_spent: 3, minutes_spent: 3 })],
      watches: {
        "bg-watch-main": {
          status: status("bg-watch-main", "failing", { last_checked_at: T + "14:02:00Z", last_exit_code: 1 }),
          // The Fri 6 Nov needs_you row is the template start that could not run: watch-failing-template draws its sentence.
          changes: [change(4, "failing", "started_template", T + "14:02:00Z", 1), recoveredYesterday, change(2, "failing", "needs_you", "2026-11-06T09:02:00Z", 1, null, "no_engine"), baseline],
        },
      },
    };
  }
  if (scenario === "crew-background-watch-needsyou" || scenario === "crew-background-watch-needsyou-p-two") {
    const redirect = watch("bg-watch-redirect", "Redirect tests", "npm test web/nav/redirect", 1, "needs_you", true);
    const lock = watch("bg-watch-lock", "Lockfile check", "npm run lockfile:check", 6, "needs_you", true);
    return {
      crews: [redirect, lock, crew("bg-docs", "Update the docs index", "fix_failing_ci", { kind: "schedule", every_hours: 24 }, true, "needs_review", 1)],
      runs: [run("r-docs-1", "bg-docs", "needs_review", "2026-11-07T13:40:00Z", "2026-11-07T14:00:00Z", { review_reason: "A command is held for your approval before the run can go on." })],
      watches: {
        "bg-watch-redirect": {
          status: status("bg-watch-redirect", "failing", { last_checked_at: T + "14:02:00Z", last_exit_code: 1, tail: FAILING_TAIL }),
          changes: [raisedAt1402, recoveredYesterday, raisedThursday, baseline],
        },
        "bg-watch-lock": {
          status: status("bg-watch-lock", "could_not_run", { last_checked_at: T + "12:02:00Z", reason: "timed out", error_streak: 3 }),
          changes: [change(2, "baseline", "none", "2026-11-05T09:10:00Z", 0)],
        },
      },
    };
  }
  return { ...base(), watches: {} };
}

const page = <T,>(key: string, items: T[], params: URLSearchParams): MockResponse => {
  const offset = Number(params.get("offset") ?? 0);
  const limit = Number(params.get("limit") ?? 50);
  const next = offset + limit < items.length ? offset + limit : null;
  return { status: 200, body: { [key]: items.slice(offset, offset + limit), total: items.length, next_offset: next } };
};
const byName = (crews: BackgroundCrew[]) => [...crews].sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()) || a.id.localeCompare(b.id));
const invalid = (message: string): MockResponse => ({ status: 400, body: { code: "invalid", message } });
const fail = (status: number, code: string, message: string): MockResponse => ({ status, body: { code, message } });

/** The commands the service's screen always denies, as its own sentence for the rule. */
const DENIED: [RegExp, string][] = [
  [/git push[^\n]*--force|push -f/, "That command is not allowed: force-push, history rewrite or any push to main."],
];
/** How many watches one project may have. */
const WATCHES_MAX = 10;

/** The route's checks of a create body, with its own messages, in its order. */
function refusal(body: BackgroundCrewCreate, watches: number): MockResponse | null {
  const name = body.name.trim();
  if (name.length < 1 || name.length > 80) return invalid("name is 1 to 80 characters");
  if ([...name].some((char) => char.charCodeAt(0) < 32)) return invalid("name has a character that cannot be shown");
  const { trigger, budget } = body;
  for (const [setting, value] of [["branch", trigger.branch], ["workflow", trigger.workflow], ["label", trigger.label], ["ecosystem", trigger.ecosystem]] as const) {
    if (value != null && (value.trim().length < 1 || value.trim().length > 100)) return invalid(`${setting} is 1 to 100 characters`);
  }
  if (trigger.command != null) {
    const command = trigger.command.trim();
    if (command.length < 1 || command.length > 500) return invalid("command is 1 to 500 characters");
    const denied = DENIED.find(([pattern]) => pattern.test(command));
    if (denied) return invalid(denied[1]);
  }
  if (trigger.every_hours != null && !(trigger.every_hours >= 1 && trigger.every_hours <= 720)) return invalid("every_hours is 1 to 720");
  if (trigger.kind === "issue_label" && trigger.label == null) return invalid("a issue_label trigger needs label");
  if (trigger.kind === "schedule" && trigger.every_hours == null) return invalid("a schedule trigger needs every_hours");
  if (trigger.kind === "watch") {
    if (trigger.command == null || trigger.on_failing == null || trigger.every_hours == null) return invalid("a watch trigger needs command, every_hours and on_failing");
    if (watches >= WATCHES_MAX) return invalid("A project can have 10 watches.");
  }
  if (!(budget.turns >= 1 && budget.turns <= 1000)) return invalid("the budget is 1 to 1000 turns");
  if (!(budget.minutes >= 1 && budget.minutes <= 1440)) return invalid("the budget is 1 to 1440 minutes");
  if (!(body.daily.runs >= 1 && body.daily.runs <= 1000)) return invalid("the daily budget is 1 to 1000 runs");
  if (!(body.daily.turns >= 1 && body.daily.turns <= 10000)) return invalid("the daily budget is 1 to 10000 turns");
  if (!(body.utc_offset_minutes >= -840 && body.utc_offset_minutes <= 840)) return invalid("utc_offset_minutes is -840 to 840");
  return null;
}

/** A watch that has never checked reads as `not_checked`: the state the service answers before the first baseline. */
const blankWatch = (id: string): BackgroundWatchStatus => ({ crew_id: id, state: "not_checked", baseline_done: false, error_streak: 0 });

export function createBackgroundMock() {
  let state = seedFor("crew-background-empty");
  let live = false;
  let ids = 0;
  const find = (id: string) => state.crews.find((one) => one.id === id);
  return {
    reset(scenario: string) {
      live = scenario.startsWith("crew-background");
      state = live ? seedFor(scenario) : { crews: [], runs: [], watches: {} };
      ids = 0;
    },
    handle(method: string, path: string, params: URLSearchParams, body: unknown): MockResponse | null {
      if (!live || !path.startsWith("/api/crew/background")) return null;
      const parts = path.split("/").slice(4);
      if (method === "GET" && parts.length === 0) return page("crews", byName(state.crews), params);
      if (method === "GET" && parts.length === 2 && parts[1] === "runs") return page("runs", state.runs.filter((one) => one.crew_id === parts[0]), params);
      // One run with its crew (KC6 "Open the run"): a run or a crew that is not stored answers 404, as the service does.
      if (method === "GET" && parts.length === 2 && parts[0] === "runs") {
        const stored = state.runs.find((one) => one.id === parts[1]);
        const owner = stored ? find(stored.crew_id) : undefined;
        if (!stored || !owner) return fail(404, "not_found", "No such background run.");
        return { status: 200, body: { run: stored, crew_id: owner.id, name: owner.name, template: owner.template } };
      }
      if (method === "GET" && parts.length === 2 && parts[1] === "watch") {
        const watched = find(parts[0]!);
        if (!watched) return fail(404, "not_found", "No such background crew.");
        if (watched.trigger.kind !== "watch") return invalid("This background crew is not a watch.");
        return { status: 200, body: state.watches[watched.id]?.status ?? blankWatch(watched.id) };
      }
      if (method === "GET" && parts.length === 3 && parts[1] === "watch" && parts[2] === "changes") {
        const watched = find(parts[0]!);
        if (!watched) return fail(404, "not_found", "No such background crew.");
        if (watched.trigger.kind !== "watch") return invalid("This background crew is not a watch.");
        return page("changes", state.watches[watched.id]?.changes ?? [], params);
      }
      if (method === "POST" && parts[0] === "create") {
        const wanted = body as BackgroundCrewCreate;
        const refused = refusal(wanted, state.crews.filter((one) => one.trigger.kind === "watch").length);
        if (refused) return refused;
        if (state.crews.some((one) => one.name.toLowerCase() === wanted.name.trim().toLowerCase())) return invalid("this project already has a background crew with that name");
        ids += 1;
        const made: BackgroundCrew = { ...crew(`bg-new-${ids}`, wanted.name.trim(), wanted.template, wanted.trigger, false, "idle", 0), project_id: wanted.project_id, budget: wanted.budget, daily: wanted.daily };
        state.crews = [made, ...state.crews];
        return { status: 200, body: made };
      }
      const found = parts[0] ? find(parts[0]) : undefined;
      if (method === "POST" && parts.length === 2 && !found) return fail(404, "not_found", "No such background crew.");
      if (method === "POST" && found && (parts[1] === "enable" || parts[1] === "disable")) {
        const updated = { ...found, enabled: parts[1] === "enable" };
        state.crews = state.crews.map((one) => (one.id === found.id ? updated : one));
        return { status: 200, body: updated };
      }
      if (method === "POST" && found && parts[1] === "update") {
        const change = body as BackgroundCrewUpdate;
        const updated = { ...found, ...(change.name ? { name: change.name } : {}), ...(change.trigger ? { trigger: change.trigger } : {}) };
        state.crews = state.crews.map((one) => (one.id === found.id ? updated : one));
        return { status: 200, body: updated };
      }
      if (method === "POST" && found && parts[1] === "delete") {
        state.crews = state.crews.filter((one) => one.id !== found.id);
        return { status: 200, body: { deleted: true } };
      }
      return null;
    },
  };
}
