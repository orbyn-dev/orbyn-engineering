// The person key (#441, docs/worker-token-isolation.md): a per-launch secret only the desktop shell holds. The page asks the
// shell for it through one Tauri command and sends it as `x-orbyn-person` on the routes that need the person, and on no other.
// It is kept in this module's memory only: never in storage, a URL or a log.
import { bridge } from "./bridge";

/** The header the service reads. */
export const PERSON_HEADER = "x-orbyn-person";

/**
 * The routes that send the key. This is the web side of `ENFORCED` in `crates/orbynd/src/routes/classes.rs`: enforcing another
 * person route there is one line, and the same line here. `personKey.test.ts` checks every entry against the service's route
 * table, so a line that names no person route fails.
 */
export const PERSON_ROUTES: readonly (readonly [method: string, route: RegExp, pattern: string])[] = [
  ["POST", /^\/api\/projects\/[^/]+\/worktrees\/[^/]+\/file-actions$/, "/api/projects/{id}/worktrees/{worktree}/file-actions"],
  ["POST", /^\/api\/approvals\/[^/]+\/decide$/, "/api/approvals/{id}/decide"],
];

/** Whether a request to this method and path carries the person key. A query string is not part of the route. */
export function sendsPersonKey(method: string, path: string): boolean {
  const route = path.split("?")[0] ?? path;
  return PERSON_ROUTES.some(([m, pattern]) => m === method.toUpperCase() && pattern.test(route));
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
