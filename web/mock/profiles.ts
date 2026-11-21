/**
 * MOCK of the profile routes of docs/execution-profiles-and-library.md, Part 1, for tests and the dev-only preview (`#/preview/profiles`),
 * which install it through `setProfilesApi`. The app's own client is the real HTTP one in src/features/profiles/api.ts. Fixtures
 * follow the Engineer frames `profile-*` and the three built-ins of the doc. It refuses what the doc says the service refuses:
 * `name_taken`, `revision_conflict`, `builtin_profile`, `profile_limit`, `confirm_required`, `engine_unavailable`, `invalid_input`.
 */
import { ApiFailure } from "../src/api/client.ts";
import { PROFILE_LIMIT, type ApplyAnswer, type ApplyBody, type Availability, type GrantCode, type Profile, type ProfileCheck, type ProfileDraft, type ProfileList, type ProfilesApi } from "../src/features/profiles/contract.ts";

const NOW = "2026-11-22T00:00:00Z";
const built = (slug: string, name: string, draft: Partial<Profile>): Profile => ({
  id: `builtin:${slug}`, name, description: "", engine: null, model_tier: "builder", permission_mode: "supervised", autopilot_mode: "off",
  builtin: true, revision: 1, created_at: NOW, updated_at: NOW, availability: { engine: "none" }, ...draft,
});

export function profileFixtures(): Profile[] {
  return [
    built("careful", "Careful", {}),
    built("fast", "Fast", { permission_mode: "auto_edits", autopilot_mode: "on" }),
    built("cheap", "Cheap", { model_tier: "scout" }),
    { id: "01j0000000000000000000a001", name: "Night shift", description: "Runs long work on Claude.", engine: "claude", model_tier: "lead", permission_mode: "auto", autopilot_mode: "on", builtin: false, revision: 3, created_at: NOW, updated_at: NOW, availability: { engine: "ok" } },
    { id: "01j0000000000000000000a002", name: "Codex builder", description: "", engine: "codex", model_tier: "builder", permission_mode: "auto", autopilot_mode: "on", builtin: false, revision: 1, created_at: NOW, updated_at: NOW, availability: { engine: "not_found", reason: "Codex is not installed." } },
    { id: "01j0000000000000000000a003", name: "Hands off", description: "", engine: null, model_tier: "builder", permission_mode: "full_access", autopilot_mode: "full", builtin: false, revision: 1, created_at: NOW, updated_at: NOW, availability: { engine: "none" } },
  ];
}

export interface ProfilesMock extends ProfilesApi {
  /** Every write the screens made, in order, such as `create:Name`, `apply:thread-1:profile-id:ack=full_access`. */
  writes: string[];
  /** Make the next write fail with this code (`engine_unavailable`, `revision_conflict`, ...). */
  failNext(code: string | null): void;
  /** Change a stored profile as if another window had saved it (bumps its revision). */
  touch(id: string): void;
}

const STATUS: Record<string, [number, string]> = {
  name_taken: [409, "A profile with that name already exists."],
  revision_conflict: [409, "This profile changed since you opened it."],
  builtin_profile: [403, "A built-in profile can’t be changed. Duplicate it to change it."],
  profile_limit: [409, "You have 50 saved profiles. Delete one to add another."],
  confirm_required: [409, "This profile needs your confirmation."],
  engine_unavailable: [409, "That engine is not available."],
  mode_not_offered: [409, "That engine does not offer this permission mode."],
  tier_unmapped: [409, "That engine has no model for this tier."],
  not_found: [404, "That profile is not in the list any more."],
  invalid_input: [400, "Check the highlighted field."],
};

export function createProfilesMock(init: { profiles?: Profile[] | null; delayMs?: number; limit?: number } = {}): ProfilesMock {
  const missing = init.profiles === null;
  const store = new Map((init.profiles ?? profileFixtures()).map((profile) => [profile.id, profile] as const));
  const writes: string[] = [];
  const limit = init.limit ?? PROFILE_LIMIT;
  let fail: string | null = null;
  let seq = 0;
  const wait = () => new Promise<void>((resolve) => setTimeout(resolve, init.delayMs ?? 0));
  const refuse = (code: string, detail?: unknown): never => {
    const [status, message] = STATUS[code] ?? [502, "The service could not do that."];
    throw new ApiFailure(code, message, status, detail);
  };
  const preflight = (): void => {
    if (fail) { const code = fail; fail = null; refuse(code); }
  };
  const availabilityOf = (engine: string | null): Availability => (engine === null ? { engine: "none" } : engine === "claude" || engine === "codex" ? { engine: "ok" } : { engine: "not_found", reason: "That engine is not installed." });
  const find = (id: string): Profile => store.get(id) ?? refuse("not_found");
  const clash = (name: string, except: string | null) => [...store.values()].some((one) => one.id !== except && one.name.trim().toLowerCase() === name.trim().toLowerCase());
  const valid = (draft: ProfileDraft) => {
    const name = draft.name.trim();
    if (name.length < 1 || name.length > 60 || (draft.permission_mode as string) === "read_only") refuse("invalid_input");
  };
  const grants = (profile: Profile): GrantCode[] => [...(profile.permission_mode === "full_access" ? (["full_access"] as const) : []), ...(profile.autopilot_mode === "full" ? (["autopilot_full"] as const) : [])];
  const apply = (scope: "thread" | "crew", id: string, body: ApplyBody): ApplyAnswer => {
    preflight();
    if (body.profile_id === null) return { applied: {}, ignored: [], application: null };
    const profile = find(body.profile_id);
    if (body.seen_revision !== undefined && body.seen_revision !== profile.revision) refuse("revision_conflict");
    const need = grants(profile);
    const seen = body.ack_grants ?? [];
    if (need.some((code) => !seen.includes(code))) refuse("confirm_required", { grants: need });
    const availability = profile.availability ?? availabilityOf(profile.engine);
    if (availability.engine !== "ok" && availability.engine !== "none") refuse("engine_unavailable");
    writes.push(`apply:${scope}:${id}:${profile.id}:ack=${seen.join("+")}`);
    return {
      applied: { ...(profile.engine ? { engine: profile.engine } : {}), model: `model-for-${profile.model_tier}`, permission_mode: profile.permission_mode, ...(scope === "crew" ? { autopilot_mode: profile.autopilot_mode } : {}) },
      ignored: scope === "thread" ? ["autopilot_mode"] : [],
      application: { profile_id: profile.id, name: profile.name, revision: profile.revision, customized: false, applied_at: NOW },
    };
  };
  return {
    writes,
    failNext(code) { fail = code; },
    touch(id) {
      const one = find(id);
      store.set(id, { ...one, revision: one.revision + 1 });
    },
    async list(): Promise<ProfileList | null> {
      await wait();
      if (missing) return null;
      return { profiles: [...store.values()].map((one) => ({ ...one })), limit };
    },
    async check(draft): Promise<ProfileCheck> {
      await wait();
      return { warnings: draft.engine !== null && availabilityOf(draft.engine).engine !== "ok" ? [{ field: "engine", code: "engine_unavailable", message: "That engine is not installed." }] : [], availability: availabilityOf(draft.engine) };
    },
    async create(draft) {
      await wait();
      preflight();
      valid(draft);
      if (clash(draft.name, null)) refuse("name_taken");
      if ([...store.values()].filter((one) => !one.builtin).length >= limit) refuse("profile_limit");
      seq += 1;
      const profile: Profile = { ...draft, name: draft.name.trim(), id: `01jnew${String(seq).padStart(20, "0")}`, builtin: false, revision: 1, created_at: NOW, updated_at: NOW, availability: availabilityOf(draft.engine) };
      store.set(profile.id, profile);
      writes.push(`create:${profile.name}`);
      return profile;
    },
    async update(id, draft, seenRevision) {
      await wait();
      preflight();
      const one = find(id);
      if (one.builtin) refuse("builtin_profile");
      valid(draft);
      if (seenRevision !== one.revision) refuse("revision_conflict");
      if (clash(draft.name, id)) refuse("name_taken");
      const next: Profile = { ...one, ...draft, name: draft.name.trim(), revision: one.revision + 1, availability: availabilityOf(draft.engine) };
      store.set(id, next);
      writes.push(`update:${next.name}`);
      return next;
    },
    async remove(id) {
      await wait();
      preflight();
      if (find(id).builtin) refuse("builtin_profile");
      store.delete(id);
      writes.push(`remove:${id}`);
    },
    async applyToThread(threadId, body) { await wait(); return apply("thread", threadId, body); },
    async applyToCrew(crewId, body) { await wait(); return apply("crew", crewId, body); },
  };
}
