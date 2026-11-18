/**
 * The mock's background crews (Crew designs: background-list and the rest): the four crews the designs draw, their runs, and the
 * writes the Background screen makes (create, on and off). It answers only in `crew-background` and `crew-background-empty`, and the
 * state is rebuilt on every reset, so a test never sees another's crews. The other Crew scenarios keep the read-only fixture of
 * `mock/needsyou.ts`.
 */
import type { BackgroundCrew } from "../src/generated/BackgroundCrew.ts";
import type { BackgroundCrewCreate } from "../src/generated/BackgroundCrewCreate.ts";
import type { BackgroundCrewUpdate } from "../src/generated/BackgroundCrewUpdate.ts";
import type { BackgroundRun } from "../src/generated/BackgroundRun.ts";
import type { MockResponse } from "./api.ts";

const budget = { turns: 40, minutes: 60 };
const crew = (id: string, name: string, template: BackgroundCrew["template"], trigger: BackgroundCrew["trigger"], enabled: boolean, state: BackgroundCrew["state"], reviews: number): BackgroundCrew => ({
  id, project_id: "p-app", name, template, trigger, budget, enabled, state, reviews_waiting: reviews, created_at: "2026-11-01T10:00:00Z", updated_at: "2026-11-07T14:00:00Z",
});
const run = (id: string, crewId: string, state: BackgroundRun["state"], started: string, ended: string | null, extra: Partial<BackgroundRun> = {}): BackgroundRun => ({
  id, crew_id: crewId, state, turns_spent: 12, minutes_spent: 6, budget, review_waiting: state === "needs_review", started_at: started, updated_at: ended ?? started, ended_at: ended, ...extra,
});

const seed = (): { crews: BackgroundCrew[]; runs: BackgroundRun[] } => ({
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

const page = <T,>(key: string, items: T[], params: URLSearchParams): MockResponse => {
  const offset = Number(params.get("offset") ?? 0);
  const limit = Number(params.get("limit") ?? 50);
  const next = offset + limit < items.length ? offset + limit : null;
  return { status: 200, body: { [key]: items.slice(offset, offset + limit), total: items.length, next_offset: next } };
};
const byName = (crews: BackgroundCrew[]) => [...crews].sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()) || a.id.localeCompare(b.id));
const invalid = (message: string): MockResponse => ({ status: 400, body: { code: "invalid", message } });
/** The route's checks of a create body, with its own messages, in its order. */
function refusal(body: BackgroundCrewCreate): MockResponse | null {
  const name = body.name.trim();
  if (name.length < 1 || name.length > 80) return invalid("name is 1 to 80 characters");
  if ([...name].some((char) => char.charCodeAt(0) < 32)) return invalid("name has a character that cannot be shown");
  const { trigger, budget } = body;
  for (const [setting, value] of [["branch", trigger.branch], ["workflow", trigger.workflow], ["label", trigger.label], ["ecosystem", trigger.ecosystem]] as const) {
    if (value != null && (value.trim().length < 1 || value.trim().length > 100)) return invalid(`${setting} is 1 to 100 characters`);
  }
  if (trigger.every_hours != null && !(trigger.every_hours >= 1 && trigger.every_hours <= 720)) return invalid("every_hours is 1 to 720");
  if (trigger.kind === "issue_label" && trigger.label == null) return invalid("a issue_label trigger needs label");
  if (trigger.kind === "schedule" && trigger.every_hours == null) return invalid("a schedule trigger needs every_hours");
  if (!(budget.turns >= 1 && budget.turns <= 1000)) return invalid("the budget is 1 to 1000 turns");
  if (!(budget.minutes >= 1 && budget.minutes <= 1440)) return invalid("the budget is 1 to 1440 minutes");
  return null;
}
const fail = (status: number, code: string, message: string): MockResponse => ({ status, body: { code, message } });

export function createBackgroundMock() {
  let state = seed();
  let live = false;
  let ids = 0;
  const find = (id: string) => state.crews.find((one) => one.id === id);
  return {
    reset(scenario: string) {
      live = scenario === "crew-background" || scenario === "crew-background-empty";
      state = scenario === "crew-background-empty" ? { crews: [], runs: [] } : seed();
      ids = 0;
    },
    handle(method: string, path: string, params: URLSearchParams, body: unknown): MockResponse | null {
      if (!live || !path.startsWith("/api/crew/background")) return null;
      const parts = path.split("/").slice(4);
      if (method === "GET" && parts.length === 0) return page("crews", byName(state.crews), params);
      if (method === "GET" && parts.length === 2 && parts[1] === "runs") return page("runs", state.runs.filter((one) => one.crew_id === parts[0]), params);
      if (method === "POST" && parts[0] === "create") {
        const wanted = body as BackgroundCrewCreate;
        const refused = refusal(wanted);
        if (refused) return refused;
        if (state.crews.some((one) => one.name.toLowerCase() === wanted.name.trim().toLowerCase())) return invalid("this project already has a background crew with that name");
        ids += 1;
        const made: BackgroundCrew = { ...crew(`bg-new-${ids}`, wanted.name.trim(), wanted.template, wanted.trigger, false, "idle", 0), project_id: wanted.project_id, budget: wanted.budget };
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
