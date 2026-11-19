import type { EscapeDetail } from "../src/generated/EscapeDetail.ts";
import type { EscapeEventRow } from "../src/generated/EscapeEventRow.ts";
import type { EscapeLog } from "../src/generated/EscapeLog.ts";
import type { EscapePossible } from "../src/generated/EscapePossible.ts";
import type { EscapeRow } from "../src/generated/EscapeRow.ts";
import type { MockResponse, Scenario } from "./api.ts";

/**
 * The escape log's mock (slice 4k): the routes the web client reads, with the Crew Designs v79 sample rows. Scenarios:
 * "crew-escapes" (six escapes: three confirmed, two possible, one dismissed, of 38 merges), "crew-escapes-empty" (14
 * merges, nothing to count), "crew-escapes-low" (6 merges, one confirmed, every group low sample),
 * "crew-escapes-unreadable" (every read answers 500 "unreadable", as a damaged log does) and "crew-escapes-stale" (the
 * first person write is refused stale, as an escape that changed since it was opened is). A write moves its escape like
 * the service does and answers with the escape as it now stands.
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

  function reset(scenario: Scenario) {
    if (scenario === "crew-escapes") {
      rows = seeds();
      engines = [
        { engine: "claude", merges: 21, possible: 1, confirmed: 2, dismissed: 1, low_sample: false },
        { engine: "codex", merges: 12, possible: 1, confirmed: 1, dismissed: 0, low_sample: false },
        { engine: "opencode", merges: 5, possible: 0, confirmed: 0, dismissed: 0, low_sample: true },
      ];
      merges = 38;
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

  /** The service answers a stale write with plain words; the same act twice changes nothing, whatever was seen. */
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
      return { status: 200, body: { project: "p-app", as_of: T + hours(17), window_days: 60, merges, low_sample: merges < 10, min_merges: 10, engines, total: all.length, rows: page.map(wire), ...counts } satisfies EscapeLog };
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
      const refuse = stale(row, seen);
      if (refuse) return refuse;
      if (suffix === "confirm") {
        if (row.state === "confirmed") return { status: 200, body: wire(row) };
        if (row.state === "dismissed") return { status: 400, body: { code: "invalid", message: "This escape is already dismissed, so it cannot be confirmed now." } };
        row.state = "confirmed";
        row.decided_by = "person";
        row.decided_unix = T + hours(17);
        row.events.push({ id: row.events.length + 1, kind: "confirmed", from_state: "possible", to_state: "confirmed", decided_by: "person", cause: "person:70", at: T + hours(17) });
        return { status: 200, body: wire(row) };
      }
      if (suffix === "dismiss") {
        if (row.state === "dismissed") return { status: 200, body: wire(row) };
        if (row.state === "confirmed") return { status: 400, body: { code: "invalid", message: "This escape is already confirmed, so it cannot be dismissed now." } };
        row.state = "dismissed";
        row.decided_by = "person";
        row.decided_unix = T + hours(17);
        row.events.push({ id: row.events.length + 1, kind: "dismissed", from_state: "possible", to_state: "dismissed", decided_by: "person", cause: "person:71", at: T + hours(17) });
        return { status: 200, body: wire(row) };
      }
      const gap = write.gap;
      if (gap !== "test_gap" && gap !== "tooling_gap") return { status: 400, body: { code: "invalid", message: "The gap is test_gap or tooling_gap." } };
      if (row.state === "dismissed") return { status: 400, body: { code: "invalid", message: "A dismissed escape’s gap cannot be set." } };
      if (row.gap === gap) return { status: 200, body: wire(row) };
      row.gap = gap;
      row.gap_by = "person";
      row.events.push({ id: row.events.length + 1, kind: "reclassified", from_state: row.state, to_state: row.state, decided_by: "person", cause: "person:72", at: T + hours(17) });
      return { status: 200, body: wire(row) };
    }
    return undefined;
  }

  return { reset, handle };
}
