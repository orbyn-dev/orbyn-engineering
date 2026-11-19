import type { EscapeDetail } from "../src/generated/EscapeDetail.ts";
import type { EscapeEventRow } from "../src/generated/EscapeEventRow.ts";
import type { EscapeLog } from "../src/generated/EscapeLog.ts";
import type { EscapePossible } from "../src/generated/EscapePossible.ts";
import type { EscapeRow } from "../src/generated/EscapeRow.ts";
import type { EscapeThresholds } from "../src/generated/EscapeThresholds.ts";
import type { MockResponse, Scenario } from "./api.ts";

/**
 * The escape log's mock (slice 4k): the routes the web client reads, with the Crew Designs v79 sample rows. Scenarios:
 * "crew-escapes" (six escapes: three confirmed, two possible, one dismissed, of 38 merges), "crew-escapes-checks" (the
 * check frames' escapes: the return-to check proposed, being written, awaiting approval, proven, the session check that
 * passed on the escaped merge too, and one withdrawn), "crew-escapes-limit" (the log answers 5,000 escapes in all, as a
 * project at the core's cap is), "crew-escapes-empty" (14 merges, nothing to count), "crew-escapes-low" (6 merges, one
 * confirmed, every group low sample), "crew-escapes-unreadable" (every read answers 500 "unreadable", as a damaged log
 * does), "crew-escapes-stale" (the first person write is refused stale, as an escape that changed since it was opened
 * is) and "crew-escapes-stored" (a person stored other thresholds, so the settings rows read their values, not the
 * defaults). A write moves its escape and its check like the service does and answers with them as they now stand.
 */

/** Sat 7 Nov 2026, 00:00 UTC: the day the design's sample rows live on. */
const T = 1_793_923_200;
const mins = (n: number) => n * 60;
const hours = (n: number) => n * 3_600;
const days = (n: number) => n * 86_400;

const SOURCE_WORDS = {
  revert: "the merge was reverted",
  person_marked: "a person marked it",
  post_merge_check: "a check failed after the merge",
  follow_up_fix: "a later fix touched the same files",
} as const;

const lineOf = (row: Pick<EscapeRow, "source" | "state">): string =>
  row.state === "possible"
    ? `Possible escape: ${SOURCE_WORDS[row.source]}. A person has not confirmed it.`
    : row.state === "confirmed"
      ? `Confirmed by a person: ${SOURCE_WORDS[row.source]}.`
      : `Dismissed: ${SOURCE_WORDS[row.source]}.`;

type Seed = Omit<EscapeRow, "line"> & { events: Omit<EscapeEventRow, "escape_id">[]; check?: EscapeDetail["check"] };

const seeds = (): Seed[] => [
  {
    id: "esc-returnto", project: "p-app", merge_task_id: "task-returnto", source: "revert", state: "confirmed",
    gap: "test_gap", gap_by: "person", engine: "claude", model: null, areas: ["web/auth"], lane: null,
    reason: "Old returnTo links stopped redirecting", found_unix: T + hours(15) + mins(34),
    decided_unix: T + hours(15) + mins(40), decided_by: "person",
    events: [
      { id: 1, kind: "detected", from_state: null, to_state: "possible", decided_by: "core", cause: "revert:e41a9c0", at: T + hours(15) + mins(34) },
      { id: 2, kind: "confirmed", from_state: "possible", to_state: "confirmed", decided_by: "person", cause: "person:40", at: T + hours(15) + mins(40) },
      { id: 3, kind: "reclassified", from_state: "confirmed", to_state: "confirmed", decided_by: "person", cause: "person:41", at: T + hours(15) + mins(41) },
      { id: 4, kind: "check_proposed", from_state: "confirmed", to_state: "confirmed", decided_by: "person", cause: "person:42", at: T + hours(15) + mins(50) },
      { id: 5, kind: "check_proven", from_state: "confirmed", to_state: "confirmed", decided_by: "core", cause: "core:task-check-1:1", at: T + hours(16) + mins(5) },
      { id: 6, kind: "check_merged", from_state: "confirmed", to_state: "confirmed", decided_by: "core", cause: "core:task-check-1:1", at: T + hours(16) + mins(22) },
    ],
    check: {
      state: "merged", path: "tests/auth/return-to.test.ts", asserts: "An old returnTo link still redirects to its path",
      run: "npm test -- tests/auth/return-to.test.ts", fix_commit: "e41a9c0a1b2c3d4e5f60718293a4b5c6d7e8f90a", task_id: "task-check-1",
      proof: { escaped_commit: "9be4410a1b2c3d4e5f60718293a4b5c6d7e8f90a", fix_commit: "e41a9c0a1b2c3d4e5f60718293a4b5c6d7e8f90a", fails_on_escape: true, passes_on_fix: true },
      note: null, attempts: 1, diff_commit: "d1c0b2a3c4d5e6f708192a3b4c5d6e7f8091a2b", diff_hash: "c".repeat(64), diff_paths: ["tests/auth/return-to.test.ts"],
      awaiting_approval: false, approved: true, line: "The check is merged and runs before every merge.",
      proposed_unix: T + hours(15) + mins(50), updated_unix: T + hours(16) + mins(22),
    },
  },
  {
    id: "esc-next", project: "p-app", merge_task_id: "task-next", source: "person_marked", state: "confirmed",
    gap: "tooling_gap", gap_by: "person", engine: "claude", model: null, areas: ["web/auth"], lane: null,
    reason: "The legacy login route lost its rate limit", found_unix: T + hours(15) + mins(20),
    decided_unix: T + hours(15) + mins(20), decided_by: "person",
    events: [
      { id: 1, kind: "detected", from_state: null, to_state: "possible", decided_by: "person", cause: "person:50", at: T + hours(15) + mins(20) },
      { id: 2, kind: "confirmed", from_state: "possible", to_state: "confirmed", decided_by: "person", cause: "person:50", at: T + hours(15) + mins(20) },
      { id: 3, kind: "reclassified", from_state: "confirmed", to_state: "confirmed", decided_by: "person", cause: "person:51", at: T + hours(15) + mins(48) },
    ],
  },
  {
    id: "esc-docs", project: "p-app", merge_task_id: "task-docs", source: "post_merge_check", state: "dismissed",
    gap: null, gap_by: null, engine: "claude", model: null, areas: ["docs/guides"], lane: null,
    reason: "A repository check failed on this device after the merge, and the merge was not reverted.",
    found_unix: T + hours(14) + mins(50), decided_unix: T + hours(15) + mins(50), decided_by: "person",
    events: [
      { id: 1, kind: "detected", from_state: null, to_state: "possible", decided_by: "core", cause: "post_merge_check:5a90e1b", at: T + hours(14) + mins(50) },
      { id: 2, kind: "dismissed", from_state: "possible", to_state: "dismissed", decided_by: "person", cause: "person:52", at: T + hours(15) + mins(50) },
    ],
  },
  {
    id: "esc-session", project: "p-app", merge_task_id: "task-session", source: "revert", state: "confirmed",
    gap: null, gap_by: null, engine: "codex", model: null, areas: ["web/auth"], lane: null,
    reason: "People were asked to sign in again after a session refresh", found_unix: T - days(4) + hours(13) + mins(15),
    decided_unix: T - days(4) + hours(13) + mins(20), decided_by: "person",
    events: [
      { id: 1, kind: "detected", from_state: null, to_state: "possible", decided_by: "core", cause: "revert:8e21d47", at: T - days(4) + hours(13) + mins(15) },
      { id: 2, kind: "confirmed", from_state: "possible", to_state: "confirmed", decided_by: "person", cause: "person:53", at: T - days(4) + hours(13) + mins(20) },
    ],
  },
  {
    id: "esc-cookie", project: "p-app", merge_task_id: "task-cookie", source: "post_merge_check", state: "possible",
    gap: null, gap_by: null, engine: "claude", model: null, areas: ["web/auth"], lane: null,
    reason: "A repository check failed on this device after the merge, and the merge was not reverted.",
    found_unix: T - days(5) + hours(16) + mins(40), decided_unix: null, decided_by: null,
    events: [{ id: 1, kind: "detected", from_state: null, to_state: "possible", decided_by: "core", cause: "post_merge_check:c7f20b9", at: T - days(5) + hours(16) + mins(40) }],
  },
  {
    id: "esc-shortcut", project: "p-app", merge_task_id: "task-shortcut", source: "follow_up_fix", state: "possible",
    gap: null, gap_by: null, engine: "codex", model: null, areas: ["web/app"], lane: null,
    reason: "A later commit, \"Fix the shortcut opening twice\", changes a line this merge added. It may be ordinary follow-up work.",
    found_unix: T - days(5) + hours(11) + mins(55), decided_unix: null, decided_by: null,
    events: [{ id: 1, kind: "detected", from_state: null, to_state: "possible", decided_by: "core", cause: "follow_up_fix:b2f1e0d", at: T - days(5) + hours(11) + mins(55) }],
  },
];

/** The check frames' escapes: one confirmed test-gap escape per check state, as the designs' samples leave them. */
const checkSeeds = (): Seed[] => [
  {
    id: "esc-proposed", project: "p-app", merge_task_id: "task-returnto", source: "revert", state: "confirmed",
    gap: "test_gap", gap_by: "person", engine: "claude", model: null, areas: ["web/auth"], lane: null,
    reason: "Old returnTo links stopped redirecting", found_unix: T + hours(15) + mins(34),
    decided_unix: T + hours(15) + mins(40), decided_by: "person",
    events: [
      { id: 1, kind: "detected", from_state: null, to_state: "possible", decided_by: "core", cause: "revert:e41a9c0", at: T + hours(15) + mins(34) },
      { id: 2, kind: "confirmed", from_state: "possible", to_state: "confirmed", decided_by: "person", cause: "person:40", at: T + hours(15) + mins(40) },
      { id: 3, kind: "reclassified", from_state: "confirmed", to_state: "confirmed", decided_by: "person", cause: "person:41", at: T + hours(15) + mins(41) },
      { id: 4, kind: "check_proposed", from_state: "confirmed", to_state: "confirmed", decided_by: "person", cause: "person:42", at: T + hours(15) + mins(50) },
    ],
    check: {
      state: "proposed", path: "tests/auth/return-to.test.ts", asserts: "An old returnTo link still redirects to its path",
      run: "npm test -- tests/auth/return-to.test.ts", fix_commit: "e41a9c0a1b2c3d4e5f60718293a4b5c6d7e8f90a", task_id: null,
      proof: null, note: null, attempts: 1, diff_commit: null, diff_hash: null, diff_paths: [],
      awaiting_approval: false, approved: false,
      line: "Proposed: add tests/auth/return-to.test.ts. Nothing runs until a person accepts.",
      proposed_unix: T + hours(15) + mins(50), updated_unix: T + hours(15) + mins(50),
    },
  },
  {
    id: "esc-writing", project: "p-app", merge_task_id: "task-returnto", source: "revert", state: "confirmed",
    gap: "test_gap", gap_by: "person", engine: "claude", model: null, areas: ["web/auth"], lane: null,
    reason: "Old returnTo links stopped redirecting", found_unix: T + hours(15) + mins(34),
    decided_unix: T + hours(15) + mins(40), decided_by: "person",
    events: [
      { id: 1, kind: "detected", from_state: null, to_state: "possible", decided_by: "core", cause: "revert:e41a9c0", at: T + hours(15) + mins(34) },
      { id: 2, kind: "confirmed", from_state: "possible", to_state: "confirmed", decided_by: "person", cause: "person:40", at: T + hours(15) + mins(40) },
      { id: 3, kind: "reclassified", from_state: "confirmed", to_state: "confirmed", decided_by: "person", cause: "person:41", at: T + hours(15) + mins(41) },
      { id: 4, kind: "check_proposed", from_state: "confirmed", to_state: "confirmed", decided_by: "person", cause: "person:42", at: T + hours(15) + mins(50) },
    ],
    check: {
      state: "writing", path: "tests/auth/return-to.test.ts", asserts: "An old returnTo link still redirects to its path",
      run: "npm test -- tests/auth/return-to.test.ts", fix_commit: "e41a9c0a1b2c3d4e5f60718293a4b5c6d7e8f90a", task_id: "task-check-2",
      proof: null, note: null, attempts: 1, diff_commit: null, diff_hash: null, diff_paths: [],
      awaiting_approval: false, approved: false, line: "A worker is writing tests/auth/return-to.test.ts.",
      proposed_unix: T + hours(15) + mins(50), updated_unix: T + hours(15) + mins(58),
    },
  },
  {
    id: "esc-proof", project: "p-app", merge_task_id: "task-returnto", source: "revert", state: "confirmed",
    gap: "test_gap", gap_by: "person", engine: "claude", model: null, areas: ["web/auth"], lane: null,
    reason: "Old returnTo links stopped redirecting", found_unix: T + hours(15) + mins(34),
    decided_unix: T + hours(15) + mins(40), decided_by: "person",
    events: [
      { id: 1, kind: "detected", from_state: null, to_state: "possible", decided_by: "core", cause: "revert:e41a9c0", at: T + hours(15) + mins(34) },
      { id: 2, kind: "confirmed", from_state: "possible", to_state: "confirmed", decided_by: "person", cause: "person:40", at: T + hours(15) + mins(40) },
      { id: 3, kind: "reclassified", from_state: "confirmed", to_state: "confirmed", decided_by: "person", cause: "person:41", at: T + hours(15) + mins(41) },
      { id: 4, kind: "check_proposed", from_state: "confirmed", to_state: "confirmed", decided_by: "person", cause: "person:42", at: T + hours(15) + mins(50) },
    ],
    check: {
      state: "writing", path: "tests/auth/return-to.test.ts", asserts: "An old returnTo link still redirects to its path",
      run: "npm test -- tests/auth/return-to.test.ts", fix_commit: "e41a9c0a1b2c3d4e5f60718293a4b5c6d7e8f90a", task_id: "task-check-2",
      proof: null, note: null, attempts: 1,
      diff_commit: "4f1c9a07d3be5a6c218e0b97f5d2a43c6e81b09d", diff_hash: "b3a7f0c95e1d482a6f07c3d9e8b15a4720c6d8f3e91b5a07c4d2f86e30a1b97c",
      diff_paths: ["tests/auth/return-to.test.ts"], awaiting_approval: true, approved: false,
      line: "Waiting for a person to approve tests/auth/return-to.test.ts (commit 4f1c9a07d3be, hash b3a7f0c95e1d): the proof runs it as you, unrestricted, so read the file first.",
      proposed_unix: T + hours(15) + mins(50), updated_unix: T + hours(16) + mins(0),
    },
  },
  {
    id: "esc-proven", project: "p-app", merge_task_id: "task-returnto", source: "revert", state: "confirmed",
    gap: "test_gap", gap_by: "person", engine: "claude", model: null, areas: ["web/auth"], lane: null,
    reason: "Old returnTo links stopped redirecting", found_unix: T + hours(15) + mins(34),
    decided_unix: T + hours(15) + mins(40), decided_by: "person",
    events: [
      { id: 1, kind: "detected", from_state: null, to_state: "possible", decided_by: "core", cause: "revert:e41a9c0", at: T + hours(15) + mins(34) },
      { id: 2, kind: "confirmed", from_state: "possible", to_state: "confirmed", decided_by: "person", cause: "person:40", at: T + hours(15) + mins(40) },
      { id: 3, kind: "reclassified", from_state: "confirmed", to_state: "confirmed", decided_by: "person", cause: "person:41", at: T + hours(15) + mins(41) },
      { id: 4, kind: "check_proposed", from_state: "confirmed", to_state: "confirmed", decided_by: "person", cause: "person:42", at: T + hours(15) + mins(50) },
      { id: 5, kind: "check_proven", from_state: "confirmed", to_state: "confirmed", decided_by: "core", cause: "core:task-check-2:1", at: T + hours(16) + mins(5) },
    ],
    check: {
      state: "proven", path: "tests/auth/return-to.test.ts", asserts: "An old returnTo link still redirects to its path",
      run: "npm test -- tests/auth/return-to.test.ts", fix_commit: "e41a9c0a1b2c3d4e5f60718293a4b5c6d7e8f90a", task_id: "task-check-2",
      proof: { escaped_commit: "9be4410a1b2c3d4e5f60718293a4b5c6d7e8f90a", fix_commit: "e41a9c0a1b2c3d4e5f60718293a4b5c6d7e8f90a", fails_on_escape: true, passes_on_fix: true },
      note: "Queued for merge like any change.", attempts: 1, diff_commit: "4f1c9a07d3be5a6c218e0b97f5d2a43c6e81b09d",
      diff_hash: "b3a7f0c95e1d482a6f07c3d9e8b15a4720c6d8f3e91b5a07c4d2f86e30a1b97c", diff_paths: ["tests/auth/return-to.test.ts"],
      awaiting_approval: false, approved: true,
      line: "Proven: tests/auth/return-to.test.ts fails on the escaped merge and passes on the fix. It goes through the merge queue like any change.",
      proposed_unix: T + hours(15) + mins(50), updated_unix: T + hours(16) + mins(5),
    },
  },
  {
    id: "esc-session", project: "p-app", merge_task_id: "task-session", source: "revert", state: "confirmed",
    gap: "test_gap", gap_by: "person", engine: "codex", model: null, areas: ["web/auth"], lane: null,
    reason: "People were asked to sign in again after a session refresh", found_unix: T - days(4) + hours(13) + mins(15),
    decided_unix: T - days(4) + hours(13) + mins(20), decided_by: "person",
    events: [
      { id: 1, kind: "detected", from_state: null, to_state: "possible", decided_by: "core", cause: "revert:8e21d47", at: T - days(4) + hours(13) + mins(15) },
      { id: 2, kind: "confirmed", from_state: "possible", to_state: "confirmed", decided_by: "person", cause: "person:53", at: T - days(4) + hours(13) + mins(20) },
      { id: 3, kind: "reclassified", from_state: "confirmed", to_state: "confirmed", decided_by: "person", cause: "person:54", at: T - days(4) + hours(13) + mins(40) },
      { id: 4, kind: "check_proposed", from_state: "confirmed", to_state: "confirmed", decided_by: "person", cause: "person:55", at: T - days(4) + hours(14) + mins(20) },
      { id: 5, kind: "check_failed", from_state: "confirmed", to_state: "confirmed", decided_by: "core", cause: "core:task-check-3:1", at: T - days(4) + hours(14) + mins(50) },
    ],
    check: {
      state: "failed", path: "tests/auth/session.test.ts", asserts: "A refreshed session keeps you signed in",
      run: "npm test -- tests/auth/session.test.ts", fix_commit: "8e21d47a1b2c3d4e5f60718293a4b5c6d7e8f90a", task_id: null,
      proof: { escaped_commit: "3be5a02a1b2c3d4e5f60718293a4b5c6d7e8f90a", fix_commit: "8e21d47a1b2c3d4e5f60718293a4b5c6d7e8f90a", fails_on_escape: false, passes_on_fix: true },
      note: null, attempts: 1, diff_commit: null, diff_hash: null, diff_paths: [],
      awaiting_approval: false, approved: false,
      line: "Refused: tests/auth/session.test.ts passes on the escaped merge too, so it proves nothing. Retry or withdraw.",
      proposed_unix: T - days(4) + hours(14) + mins(20), updated_unix: T - days(4) + hours(14) + mins(50),
    },
  },
  {
    id: "esc-withdrawn", project: "p-app", merge_task_id: "task-returnto", source: "revert", state: "confirmed",
    gap: "test_gap", gap_by: "person", engine: "claude", model: null, areas: ["web/auth"], lane: null,
    reason: "Old returnTo links stopped redirecting", found_unix: T + hours(15) + mins(34),
    decided_unix: T + hours(15) + mins(40), decided_by: "person",
    events: [
      { id: 1, kind: "detected", from_state: null, to_state: "possible", decided_by: "core", cause: "revert:e41a9c0", at: T + hours(15) + mins(34) },
      { id: 2, kind: "confirmed", from_state: "possible", to_state: "confirmed", decided_by: "person", cause: "person:40", at: T + hours(15) + mins(40) },
      { id: 3, kind: "reclassified", from_state: "confirmed", to_state: "confirmed", decided_by: "person", cause: "person:41", at: T + hours(15) + mins(41) },
      { id: 4, kind: "check_proposed", from_state: "confirmed", to_state: "confirmed", decided_by: "person", cause: "person:42", at: T + hours(15) + mins(50) },
      { id: 5, kind: "check_withdrawn", from_state: "confirmed", to_state: "confirmed", decided_by: "person", cause: "person:56", at: T + hours(16) + mins(10) },
    ],
    check: {
      state: "withdrawn", path: "tests/auth/return-to.test.ts", asserts: "An old returnTo link still redirects to its path",
      run: "npm test -- tests/auth/return-to.test.ts", fix_commit: "e41a9c0a1b2c3d4e5f60718293a4b5c6d7e8f90a", task_id: null,
      proof: null, note: null, attempts: 1, diff_commit: null, diff_hash: null, diff_paths: [],
      awaiting_approval: false, approved: false, line: "Withdrawn by a person.",
      proposed_unix: T + hours(15) + mins(50), updated_unix: T + hours(16) + mins(10),
    },
  },
];

/** The low-sample repo's one escape: "Update the calendar feed token", marked by you. */
const lowSeed = (): Seed[] => [
  {
    id: "esc-feed", project: "p-app", merge_task_id: "task-feed", source: "person_marked", state: "confirmed",
    gap: "test_gap", gap_by: "person", engine: "claude", model: null, areas: ["web/feed"], lane: null,
    reason: "The old feed token still worked after it was replaced", found_unix: T + hours(10) + mins(15),
    decided_unix: T + hours(10) + mins(15), decided_by: "person",
    events: [
      { id: 1, kind: "detected", from_state: null, to_state: "possible", decided_by: "person", cause: "person:60", at: T + hours(10) + mins(15) },
      { id: 2, kind: "confirmed", from_state: "possible", to_state: "confirmed", decided_by: "person", cause: "person:60", at: T + hours(10) + mins(15) },
      { id: 3, kind: "reclassified", from_state: "confirmed", to_state: "confirmed", decided_by: "person", cause: "person:61", at: T + hours(10) + mins(30) },
    ],
  },
];

const UNREADABLE = { code: "unreadable", message: "The escape log on this device can’t be read." };

export function createEscapesMock() {
  let rows: (Seed & { line?: string })[] = [];
  let engines: EscapeLog["engines"] = [];
  let merges = 0;
  let unreadable = false;
  let staleOnce = false;
  let atLimit = false;
  // The thresholds the service would read from its settings table: the defaults, or a person's stored values.
  let thresholds: EscapeThresholds = { report_days: 60, follow_up_days: 14, min_merges: 10, rate_multiple: 2, stored: false };

  function reset(scenario: Scenario) {
    if (scenario === "crew-escapes" || scenario === "crew-escapes-checks" || scenario === "crew-escapes-limit") {
      rows = scenario === "crew-escapes-checks" ? checkSeeds() : seeds();
      engines = [
        { engine: "claude", merges: 21, possible: 1, confirmed: 2, dismissed: 1, low_sample: false },
        { engine: "codex", merges: 12, possible: 1, confirmed: 1, dismissed: 0, low_sample: false },
        { engine: "opencode", merges: 5, possible: 0, confirmed: 0, dismissed: 0, low_sample: true },
      ];
      merges = 38;
      atLimit = scenario === "crew-escapes-limit";
    } else if (scenario === "crew-escapes-low") {
      rows = lowSeed();
      engines = [
        { engine: "claude", merges: 4, possible: 0, confirmed: 1, dismissed: 0, low_sample: true },
        { engine: "codex", merges: 2, possible: 0, confirmed: 0, dismissed: 0, low_sample: true },
      ];
      merges = 6;
    } else {
      rows = [];
      engines = [
        { engine: "claude", merges: 9, possible: 0, confirmed: 0, dismissed: 0, low_sample: true },
        { engine: "codex", merges: 5, possible: 0, confirmed: 0, dismissed: 0, low_sample: true },
      ];
      merges = 14;
    }
    unreadable = scenario === "crew-escapes-unreadable";
    staleOnce = scenario === "crew-escapes-stale";
    thresholds =
      scenario === "crew-escapes-stored"
        ? { report_days: 30, follow_up_days: 21, min_merges: 20, rate_multiple: 3, stored: true }
        : { report_days: 60, follow_up_days: 14, min_merges: 10, rate_multiple: 2, stored: false };
  }

  const wire = (row: Seed & { line?: string }): EscapeRow => {
    const { events: _events, check: _check, ...rest } = row;
    return { ...rest, line: lineOf(row) };
  };

  const project = (search: URLSearchParams): MockResponse | null =>
    search.get("project") === "p-app" ? null : { status: 404, body: { code: "not_found", message: "No such project on this device." } };

  const totals = () => ({
    possible: rows.filter((row) => row.state === "possible").length,
    confirmed: rows.filter((row) => row.state === "confirmed").length,
    dismissed: rows.filter((row) => row.state === "dismissed").length,
  });

  /** The service answers a stale write with plain words. It answers an unchanged repeat before it checks stale, so callers check that first. */
  const stale = (row: Seed, seen: { state: string; gap?: string | null }) => {
    if (seen.state !== row.state || (seen.gap ?? null) !== (row.gap ?? null)) {
      return { status: 409, body: { code: "stale", message: "This escape changed since you opened it." } };
    }
    return null;
  };

  function handle(method: string, pathname: string, search: URLSearchParams, body: unknown): MockResponse | undefined {
    if (!pathname.startsWith("/api/crew/escapes")) return undefined;
    const id = /^\/api\/crew\/escapes\/([A-Za-z0-9_.:-]+)(?:\/(confirm|dismiss|gap))?$/.exec(pathname)?.[1] ?? "";
    const suffix = /^\/api\/crew\/escapes\/([A-Za-z0-9_.:-]+)(?:\/(confirm|dismiss|gap))?$/.exec(pathname)?.[2] ?? "";
    if (method === "GET" && pathname === "/api/crew/escapes") {
      const known = project(search);
      if (known) return known;
      if (unreadable) return { status: 500, body: UNREADABLE };
      const state = search.get("state");
      const all = rows.filter((row) => !state || row.state === state).sort((a, b) => b.found_unix - a.found_unix);
      const limit = Math.min(Number(search.get("limit") ?? 50), 200);
      const offset = Number(search.get("offset") ?? 0);
      const page = all.slice(offset, offset + limit);
      const counts = totals();
      // A project at the core's cap: the count in all is the limit, and the page rows are still the newest page.
      const total = atLimit ? 5000 : all.length;
      return { status: 200, body: { project: "p-app", as_of: T + hours(17), window_days: 60, merges, low_sample: merges < 10, min_merges: 10, engines, total, rows: page.map(wire), ...counts } satisfies EscapeLog };
    }
    if (method === "GET" && pathname === "/api/crew/escapes/thresholds") {
      if (unreadable) return { status: 500, body: UNREADABLE };
      return { status: 200, body: { ...thresholds } satisfies EscapeThresholds };
    }
    if (method === "GET" && pathname === "/api/crew/escapes/possible") {
      const known = project(search);
      if (known) return known;
      if (unreadable) return { status: 500, body: UNREADABLE };
      const all = rows.filter((row) => row.state === "possible").sort((a, b) => b.found_unix - a.found_unix);
      return { status: 200, body: { project: "p-app", as_of: T + hours(17), total: all.length, rows: all.map(wire) } satisfies EscapePossible };
    }
    if (method === "GET" && !suffix && id) {
      if (unreadable) return { status: 500, body: UNREADABLE };
      const row = rows.find((one) => one.id === id);
      if (!row) return { status: 404, body: { code: "not_found", message: "No such escape on this device." } };
      return { status: 200, body: { escape: wire(row), events: row.events, truncated: false, check: row.check ?? null } satisfies EscapeDetail };
    }
    // The five check routes: a write moves the check like the service does and answers with the escape and its check.
    const checkPath = /^\/api\/crew\/escapes\/([A-Za-z0-9_.:-]+)\/check(\/(accept|approve|retry|withdraw))?$/.exec(pathname);
    if (method === "POST" && checkPath) {
      const write = (body ?? {}) as Record<string, unknown>;
      if (write.project !== "p-app") return { status: 404, body: { code: "not_found", message: "No such escape on this device." } };
      const row = rows.find((one) => one.id === checkPath[1]);
      if (!row) return { status: 404, body: { code: "not_found", message: "No such escape on this device." } };
      if (staleOnce) {
        staleOnce = false;
        return { status: 409, body: { code: "stale", message: "This check changed since you looked. Reload it and choose again." } };
      }
      const seen = (write.seen ?? {}) as { state: string; gap?: string | null; check?: string | null };
      if (seen.state !== row.state || (seen.gap ?? null) !== (row.gap ?? null) || (seen.check ?? null) !== (row.check?.state ?? null)) {
        return { status: 409, body: { code: "stale", message: "This check changed since you looked. Reload it and choose again." } };
      }
      const kind = checkPath[3] ?? "";
      const bad = (message: string) => ({ status: 400, body: { code: "invalid", message } });
      const push = (kind2: string, cause: string) =>
        row.events.push({ id: row.events.length + 1, kind: kind2 as Seed["events"][number]["kind"], from_state: row.state, to_state: row.state, decided_by: "person", cause, at: T + hours(17) });
      const answer = () => ({ status: 200, body: { escape: wire(row), check: row.check ?? null } });
      if (kind === "accept") {
        if (!row.check) return bad("This escape has no check to accept.");
        if (row.check.state !== "proposed") return bad("This check is not proposed, so it cannot be accepted now.");
        if (row.state !== "confirmed") return bad("Only a confirmed escape can have a check written.");
        row.check.state = "writing";
        row.check.task_id = "task-check-2";
        row.check.attempts += 1;
        row.check.line = `A worker is writing ${row.check.path}.`;
        row.check.updated_unix = T + hours(17);
        return answer();
      }
      if (kind === "approve") {
        if (!row.check || row.check.state !== "writing" || !row.check.awaiting_approval) return bad("This check is not waiting for an approval.");
        if (write.commit !== row.check.diff_commit || write.diff_hash !== row.check.diff_hash)
          return { status: 409, body: { code: "stale", message: "This check changed since you looked. Reload it and choose again." } };
        row.check.approved = true;
        row.check.awaiting_approval = false;
        row.check.line = `${row.check.path} was approved; the proof is running.`;
        row.check.updated_unix = T + hours(17);
        return answer();
      }
      if (kind === "retry") {
        if (!row.check) return bad("This escape has no check to retry.");
        if (row.check.state !== "failed") return bad("This check is not failed, so it cannot be retried now.");
        row.check.state = "proposed";
        row.check.task_id = null;
        row.check.proof = null;
        row.check.note = null;
        row.check.line = `Proposed: add ${row.check.path}. Nothing runs until a person accepts.`;
        row.check.updated_unix = T + hours(17);
        push("check_proposed", "retry:82");
        return answer();
      }
      if (kind === "withdraw") {
        if (!row.check) return bad("This escape has no check to withdraw.");
        if (row.check.state === "merged") return bad("This check is merged, so it cannot be withdrawn now.");
        if (row.check.state === "withdrawn") return answer();
        row.check.state = "withdrawn";
        row.check.line = "Withdrawn by a person.";
        row.check.updated_unix = T + hours(17);
        push("check_withdrawn", "person:83");
        return answer();
      }
      // A proposal: the body names the one test file, what it asserts and how it runs.
      if (row.state !== "confirmed") return bad("Only a confirmed escape can have a check proposed.");
      if (row.gap === "tooling_gap") return bad("A tooling gap has no proposal in this version. Change the gap to a test gap to propose a check.");
      if (row.check && row.check.state !== "withdrawn") return bad("This escape already has a check. Withdraw it before proposing another.");
      const path = String(write.path ?? "");
      const asserts = String(write.asserts ?? "");
      const run = String(write.run ?? "");
      if (!path || !asserts || !run) return bad("The proposal names the test file, what it asserts and how it runs.");
      row.check = {
        state: "proposed", path, asserts, run,
        fix_commit: (write.fix_commit as string | undefined) ?? "e41a9c0a1b2c3d4e5f60718293a4b5c6d7e8f90a",
        task_id: null, proof: null, note: null, attempts: 0, diff_commit: null, diff_hash: null, diff_paths: [],
        awaiting_approval: false, approved: false,
        line: `Proposed: add ${path}. Nothing runs until a person accepts.`,
        proposed_unix: T + hours(17), updated_unix: T + hours(17),
      };
      push("check_proposed", "person:84");
      return answer();
    }
    if (method === "POST" && suffix) {
      const write = (body ?? {}) as Record<string, unknown>;
      if (write.project !== "p-app") return { status: 404, body: { code: "not_found", message: "No such escape on this device." } };
      const row = rows.find((one) => one.id === id);
      if (!row) return { status: 404, body: { code: "not_found", message: "No such escape on this device." } };
      if (staleOnce) {
        // The scenario's one refusal: the first person write is stale, as an escape that changed in another window is.
        staleOnce = false;
        return { status: 409, body: { code: "stale", message: "This escape changed since you opened it." } };
      }
      const seen = (write.seen ?? {}) as { state: string; gap?: string | null };
      if (suffix === "confirm") {
        if (row.state === "confirmed") return { status: 200, body: wire(row) };
        const refuse = stale(row, seen);
        if (refuse) return refuse;
        if (row.state === "dismissed") return { status: 400, body: { code: "invalid", message: "This escape is already dismissed, so it cannot be confirmed now." } };
        row.state = "confirmed";
        row.decided_by = "person";
        row.decided_unix = T + hours(17);
        row.events.push({ id: row.events.length + 1, kind: "confirmed", from_state: "possible", to_state: "confirmed", decided_by: "person", cause: "person:70", at: T + hours(17) });
        return { status: 200, body: wire(row) };
      }
      if (suffix === "dismiss") {
        if (row.state === "dismissed") return { status: 200, body: wire(row) };
        const refuse = stale(row, seen);
        if (refuse) return refuse;
        if (row.state === "confirmed") return { status: 400, body: { code: "invalid", message: "This escape is already confirmed, so it cannot be dismissed now." } };
        row.state = "dismissed";
        row.decided_by = "person";
        row.decided_unix = T + hours(17);
        row.events.push({ id: row.events.length + 1, kind: "dismissed", from_state: "possible", to_state: "dismissed", decided_by: "person", cause: "person:71", at: T + hours(17) });
        return { status: 200, body: wire(row) };
      }
      const gap = write.gap;
      if (gap !== "test_gap" && gap !== "tooling_gap") return { status: 400, body: { code: "invalid", message: "The gap is test_gap or tooling_gap." } };
      if (row.gap === gap && row.gap_by === "person") return { status: 200, body: wire(row) };
      const refuse = stale(row, seen);
      if (refuse) return refuse;
      if (row.state === "dismissed") return { status: 400, body: { code: "invalid", message: "A dismissed escape’s gap cannot be set." } };
      row.gap = gap;
      row.gap_by = "person";
      row.events.push({ id: row.events.length + 1, kind: "reclassified", from_state: row.state, to_state: row.state, decided_by: "person", cause: "person:72", at: T + hours(17) });
      return { status: 200, body: wire(row) };
    }
    return undefined;
  }

  return { reset, handle };
}
