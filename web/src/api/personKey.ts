// The person key (#441, docs/worker-token-isolation.md): a per-launch secret only the desktop shell holds. The page asks the
// shell for it through one Tauri command and sends it as `x-orbyn-person` on the routes that need the person, and on no other.
// It is kept in this module's memory only: never in storage, a URL or a log.
import { bridge } from "./bridge";

/** The header the service reads. */
export const PERSON_HEADER = "x-orbyn-person";

/**
 * The routes that send the key: the web side of `ENFORCED` and `FIELD_LEVEL` in `crates/orbynd/src/routes/classes.rs`. Enforcing
 * another person route there is one line, and the same line here. `personKey.test.ts` checks that this list is exactly the
 * service's person rows plus its field-level rows, so a line that names no such route, or a missing one, fails. The terminal
 * sockets (`GET /ws/terminal/...`) are in the list for that check, but they open with `new WebSocket`, which cannot set a
 * header: they offer the key as a subprotocol (`personKeyProtocols`).
 */
export const PERSON_ROUTES: readonly (readonly [method: string, pattern: string])[] = [
  // Account (behind its own local_only layer).
  ["POST", "/api/account/approvals/{id}/approve"],
  ["POST", "/api/account/approvals/{id}/deny"],
  ["POST", "/api/account/confirm"],
  ["POST", "/api/account/confirm/cancel"],
  ["POST", "/api/account/connect"],
  ["POST", "/api/account/connect/later"],
  ["POST", "/api/account/connect/seen"],
  ["POST", "/api/account/connect/use-account"],
  ["POST", "/api/account/connect/use-device"],
  ["DELETE", "/api/account/devices/{id}"],
  ["POST", "/api/account/ended/seen"],
  ["POST", "/api/account/sign-in"],
  ["POST", "/api/account/sign-in/approval"],
  ["POST", "/api/account/sign-in/cancel"],
  ["POST", "/api/account/sign-in/digits"],
  ["POST", "/api/account/sign-in/second-factor"],
  ["POST", "/api/account/sign-out"],
  ["POST", "/api/account/sign-out-everywhere"],
  ["POST", "/api/account/sign-up/code"],
  ["POST", "/api/account/sign-up/invite"],
  ["POST", "/api/account/sign-up/resend"],
  ["POST", "/api/account/sign-up/start"],
  ["POST", "/api/account/sync/now"],
  ["POST", "/api/account/sync/pause"],
  ["POST", "/api/account/sync/resume"],
  // Crew person writes (behind local_only).
  ["POST", "/api/crew/backtest/start"],
  ["POST", "/api/crew/backtest/{id}/stop"],
  ["POST", "/api/crew/capacity/settings"],
  ["POST", "/api/crew/capacity/tasks/{id}"],
  ["POST", "/api/crew/approvals/second-engine/{id}/dismiss"],
  ["POST", "/api/crew/approvals/{id}/approve-once"],
  ["POST", "/api/crew/approvals/{id}/deny"],
  ["POST", "/api/crew/approvals/{id}/dismiss"],
  ["POST", "/api/crew/autopilot/areas/follow"],
  ["POST", "/api/crew/autopilot/areas/override"],
  ["POST", "/api/crew/autopilot/confirm"],
  ["PUT", "/api/crew/autopilot/mode"],
  ["POST", "/api/crew/autopilot/review/{task_id}/comment"],
  ["POST", "/api/crew/autopilot/review/{task_id}/looks-good"],
  ["POST", "/api/crew/autopilot/review/{task_id}/revert"],
  ["POST", "/api/crew/background/create"],
  ["POST", "/api/crew/background/{id}/delete"],
  ["POST", "/api/crew/background/{id}/disable"],
  ["POST", "/api/crew/background/{id}/enable"],
  ["POST", "/api/crew/background/{id}/update"],
  ["POST", "/api/crew/guard/targets/add"],
  ["POST", "/api/crew/guard/targets/{id}/remove"],
  ["POST", "/api/crew/escapes/mark"],
  ["POST", "/api/crew/escapes/{id}/confirm"],
  ["POST", "/api/crew/escapes/{id}/dismiss"],
  ["POST", "/api/crew/escapes/{id}/gap"],
  ["POST", "/api/crew/projects/{id}/pinned-ref/confirm"],
  ["POST", "/api/crew/projects/{id}/pinned-ref/reset"],
  ["POST", "/api/crew/projects/{id}/worker-provider/set"],
  ["POST", "/api/crew/review/{task_id}/consent"],
  ["POST", "/api/crew/review/{task_id}/remove-lane"],
  ["POST", "/api/crew/settings/engine-tiers/save"],
  ["POST", "/api/crew/tasks/{id}/cancel"],
  ["POST", "/api/crew/tasks/{id}/merge"],
  ["POST", "/api/crew/tasks/{id}/rerun-checks"],
  ["POST", "/api/crew/tasks/{id}/retry-elsewhere"],
  // Engines, updates, workspace and the shell.
  ["POST", "/api/acp/registry/{id}"],
  ["POST", "/api/engines/custom"],
  ["POST", "/api/engines/probe-binary"],
  ["POST", "/api/engines/update-all"],
  ["POST", "/api/engines/{id}/install-recommended"],
  ["POST", "/api/engines/{id}/update"],
  ["POST", "/api/shell/actions"],
  ["POST", "/api/updates/app/apply"],
  ["POST", "/api/updates/apply-all"],
  ["POST", "/api/workspace/rollback"],
  ["POST", "/api/workspace/setup"],
  ["POST", "/api/workspace/update"],
  // Preferences, policy and redaction.
  ["POST", "/api/policy/rules"],
  ["DELETE", "/api/policy/rules/{id}"],
  ["PUT", "/api/preferences"],
  ["PUT", "/api/projects/{id}/preferences"],
  ["PUT", "/api/settings/preferences"],
  ["PUT", "/api/settings/secrets/formats/{id}"],
  ["POST", "/api/settings/secrets/patterns"],
  ["DELETE", "/api/settings/secrets/patterns/{id}"],
  ["PUT", "/api/settings/secrets/patterns/{id}"],
  ["PUT", "/api/settings/secrets/projects/{id}"],
  ["PUT", "/api/settings/secrets/surfaces"],
  // Projects, approvals and questions.
  ["POST", "/api/approvals/{id}/decide"],
  ["POST", "/api/projects/{id}/publish"],
  ["POST", "/api/projects/{id}/pulls/close"],
  ["PATCH", "/api/projects/{id}/pulls/{number}"],
  ["POST", "/api/projects/{id}/pulls/{number}/action"],
  ["POST", "/api/projects/{id}/pulls/{number}/comments"],
  ["PATCH", "/api/projects/{id}/pulls/{number}/comments/{comment}"],
  ["POST", "/api/projects/{id}/pulls/{number}/comments/{comment}/react"],
  ["POST", "/api/projects/{id}/pulls/{number}/labels"],
  ["POST", "/api/projects/{id}/pulls/{number}/reviewers"],
  ["POST", "/api/projects/{id}/pulls/{number}/stack"],
  ["POST", "/api/projects/{id}/pulls/{number}/threads/{thread}/resolve"],
  ["POST", "/api/projects/{id}/pulls/{number}/merge"],
  ["POST", "/api/projects/{id}/pulls/{number}/review"],
  ["POST", "/api/projects/{id}/trust"],
  ["POST", "/api/projects/{id}/workflows/{run}/approve"],
  ["POST", "/api/questions/{id}/answer"],
  ["POST", "/api/questions/{id}/dismiss"],
  // Scheduled tasks.
  ["POST", "/api/tasks"],
  ["PUT", "/api/tasks/{id}"],
  ["POST", "/api/tasks/{id}/enabled"],
  // Terminal sockets: the key travels as a subprotocol, never in the URL.
  ["GET", "/ws/terminal/{thread_id}"],
  ["GET", "/ws/terminal/{thread_id}/{terminal}"],
  // Threads: console, terminals, scripts, files, computer use, pull requests, permission mode.
  ["PATCH", "/api/threads/{id}"],
  ["POST", "/api/threads/{id}/computer"],
  ["POST", "/api/threads/{id}/console"],
  ["PUT", "/api/threads/{id}/files/content"],
  ["POST", "/api/threads/{id}/pulls/{number}/merge"],
  ["POST", "/api/threads/{id}/pulls/{number}/review"],
  ["POST", "/api/threads/{id}/scripts"],
  ["POST", "/api/threads/{id}/terminals"],
  // Field-level routes (`FIELD_LEVEL` in classes.rs): `Local` as a whole, and the service reads the key for part of the body only.
  // File actions: the key is read for every action except a plain read.
  ["POST", "/api/projects/{id}/worktrees/{worktree}/file-actions"],
  // Runs and AG-UI resume: only when the body raises `permission_mode` (or has a non-empty `resume`).
  ["POST", "/api/threads/{id}/runs"],
  ["POST", "/agui/threads/{id}"],
];

/** The service's route pattern as a matcher: each `{name}` is one non-empty path segment. */
function matcher(pattern: string): RegExp {
  const source = pattern
    .split(/\{[^}]+\}/)
    .map((part) => part.replace(/[.*+?^${}()|[\]\\/]/g, "\\$&"))
    .join("[^/]+");
  return new RegExp(`^${source}$`);
}

const COMPILED = PERSON_ROUTES.map(([method, pattern]) => [method, matcher(pattern)] as const);

/** Whether a request to this method and path carries the person key. A query string is not part of the route. */
export function sendsPersonKey(method: string, path: string): boolean {
  const route = path.split("?")[0] ?? path;
  return COMPILED.some(([m, pattern]) => m === method.toUpperCase() && pattern.test(route));
}

/**
 * What the dev server's mock and the unit tests send where there is no desktop shell: a fixed fake key. `__ORBYN_PREVIEWS__` is
 * false in a production build, so this branch and the string are not in the shipped page, which has the shell or no key.
 */
const MOCK_KEY = "mock-person-key";

let cached: string | null = null;

/** Forgets the key, so the next person request asks the shell again (the service restarted, or the key was refused). */
export function forgetPersonKey(): void {
  cached = null;
}

/**
 * The key, from the shell's `person_key` command, or null when this page has no shell (a browser tab) or the shell has no
 * key. Without one the request goes out without the header, and the service answers 403 `desktop_required`.
 */
export async function readPersonKey(): Promise<string | null> {
  if (cached) return cached;
  const invoke = bridge()?.invoke;
  if (typeof invoke !== "function") return __ORBYN_PREVIEWS__ ? MOCK_KEY : null;
  let answer: unknown;
  try {
    answer = await invoke.call(bridge(), "person_key");
  } catch {
    return null;
  }
  if (typeof answer !== "string" || answer === "" || answer.length > 256) return null;
  cached = answer;
  return cached;
}

/** The headers to add to one request: the key for a person route, nothing for any other. */
export async function personKeyHeaders(method: string, path: string): Promise<Record<string, string>> {
  if (!sendsPersonKey(method, path)) return {};
  const key = await readPersonKey();
  return key ? { [PERSON_HEADER]: key } : {};
}

/** The subprotocols a terminal socket opens with: the one the service selects, and the key as `orbyn.person.<key>`. The key is never in the URL. */
export async function personKeyProtocols(): Promise<string[]> {
  const key = await readPersonKey();
  return key ? ["orbyn.v1", `orbyn.person.${key}`] : ["orbyn.v1"];
}
