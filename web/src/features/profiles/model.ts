import { DESCRIPTION_MAX, NAME_MAX, type Availability, type AutopilotModeId, type GrantCode, type ModelTier, type PermissionModeId, type Profile, type ProfileDraft } from "./contract";

/** Text the service or the person chose: cut so no line can grow a row. React escapes it; it is never markup. */
export const plain = (text: unknown, max = 80): string => {
  const whole = typeof text === "string" ? text : "";
  return whole.length > max ? `${whole.slice(0, max)}…` : whole;
};

// Labels are read from fixed maps: a word the service adds later is shown as it came, cut, and is never an own-property lookup on a plain object.
const label = (names: ReadonlyMap<string, string>, key: unknown, fallback: string): string => (typeof key === "string" ? (names.get(key) ?? plain(key, 24) ) : fallback);

const TIERS = new Map<string, string>([["lead", "Lead"], ["builder", "Builder"], ["scout", "Scout"]]);
const MODES = new Map<string, string>([["supervised", "Supervised"], ["auto_edits", "Auto edits"], ["auto", "Auto"], ["full_access", "Full access"]]);
const AUTOPILOT = new Map<string, string>([["off", "Off"], ["on", "On"], ["full", "Full"]]);
export const tierLabel = (tier: ModelTier | string): string => label(TIERS, tier, "Unknown");
export const modeLabel = (mode: PermissionModeId | string): string => label(MODES, mode, "Unknown");
export const autopilotLabel = (mode: AutopilotModeId | string): string => label(AUTOPILOT, mode, "Unknown");

/** What each permission mode means, in the words of the Engineer frames. */
export const MODE_HINTS: ReadonlyMap<string, string> = new Map([
  ["supervised", "Ask before commands and file changes."],
  ["auto_edits", "Edits files without asking; asks before commands."],
  ["auto", "Runs what it judges safe without asking."],
  ["full_access", "Runs any command without asking."],
]);
export const AUTOPILOT_HINTS: ReadonlyMap<string, string> = new Map([
  ["off", "Crew waits for you."],
  ["on", "Crew merges low-risk work."],
  ["full", "High risk waits for you before merging."],
]);
export const TIERS_IN_ORDER: readonly ModelTier[] = ["lead", "builder", "scout"];
export const MODES_IN_ORDER: readonly PermissionModeId[] = ["supervised", "auto_edits", "auto", "full_access"];
export const AUTOPILOT_IN_ORDER: readonly AutopilotModeId[] = ["off", "on", "full"];

/** The grants a profile carries: `full_access` for the permission mode and `autopilot_full` for Autopilot. Careful, Fast and Cheap carry none. */
export function grantsOf(profile: Pick<Profile, "permission_mode" | "autopilot_mode">): GrantCode[] {
  const grants: GrantCode[] = [];
  if (profile.permission_mode === "full_access") grants.push("full_access");
  if (profile.autopilot_mode === "full") grants.push("autopilot_full");
  return grants;
}

const GRANT_TEXT = new Map<string, string>([
  ["full_access", "Full access: runs any command without asking."],
  ["autopilot_full", "Autopilot Full: merges when checks pass."],
]);
export const grantText = (code: string): string => GRANT_TEXT.get(code) ?? "A grant this app does not know. Check the profile before you apply it.";

/** Grant codes the service named on a `confirm_required`, read defensively: only the codes this app knows, once each. */
export function grantsFromFailure(detail: unknown): GrantCode[] {
  const raw = typeof detail === "object" && detail !== null && Array.isArray((detail as { grants?: unknown }).grants) ? (detail as { grants: unknown[] }).grants : [];
  const known = raw.filter((code): code is GrantCode => code === "full_access" || code === "autopilot_full");
  return [...new Set(known)];
}

/** The reason an engine cannot run a profile, for its row. `null` when nothing needs saying. */
export function availabilityText(availability: Availability | undefined, engineName: string): { text: string; fix: boolean } | null {
  if (!availability) return null;
  const reason = plain(availability.reason, 120);
  switch (availability.engine) {
    case "ok":
    case "none":
      return null;
    case "not_found":
      return { text: reason || `${engineName} is not installed.`, fix: true };
    case "cannot_start":
      return { text: reason || `${engineName} can’t start.`, fix: true };
    case "disabled":
      return { text: reason || `${engineName} is turned off.`, fix: true };
    case "unmanaged":
      return { text: reason || `${engineName} is not managed by Orbyn.`, fix: false };
    default:
      return { text: reason || "Orbyn can’t tell whether this engine runs.", fix: false };
  }
}

/** The pill on a row: the short label the doc gives ("Engine not found", "Can't start"). */
export function availabilityPill(availability: Availability | undefined): string | null {
  switch (availability?.engine) {
    case "not_found": return "Engine not found";
    case "cannot_start": return "Can’t start";
    case "disabled": return "Engine off";
    default: return null;
  }
}

/** A profile whose engine is not startable is shown disabled with its reason. Orbyn never falls back to another engine. */
export const usable = (profile: Pick<Profile, "availability">): boolean => {
  const kind = profile.availability?.engine;
  return kind === undefined || kind === "ok" || kind === "none" || kind === "unmanaged";
};

const hasControl = (text: string): boolean => Array.from(text).some((char) => { const code = char.charCodeAt(0); return code < 32 || (code >= 127 && code <= 159); });

export interface DraftProblems { name?: string; description?: string }

/** The doc's field rules, checked before a request so the person's text is kept and Save stays off. The service checks again. */
export function checkDraft(draft: ProfileDraft, others: readonly Pick<Profile, "id" | "name">[], selfId: string | null): DraftProblems {
  const problems: DraftProblems = {};
  const name = draft.name.trim();
  if (name.length === 0) problems.name = "Give the profile a name.";
  else if (name.length > NAME_MAX) problems.name = `Use at most ${NAME_MAX} characters.`;
  else if (hasControl(name)) problems.name = "Use letters, numbers and punctuation only.";
  else {
    const key = name.toLowerCase();
    const taken = others.find((one) => one.id !== selfId && one.name.trim().toLowerCase() === key);
    if (taken) problems.name = `A profile called ${plain(taken.name, 40)} already exists. Choose another name.`;
  }
  if (draft.description.length > DESCRIPTION_MAX) problems.description = `Use at most ${DESCRIPTION_MAX} characters.`;
  else if (hasControl(draft.description.replace(/[\n\t]/g, " "))) problems.description = "Use letters, numbers and punctuation only.";
  return problems;
}

export const draftOf = (profile: Profile): ProfileDraft => ({
  name: profile.name,
  description: profile.description,
  engine: profile.engine,
  model_tier: profile.model_tier,
  permission_mode: profile.permission_mode,
  autopilot_mode: profile.autopilot_mode,
});

export const emptyDraft = (): ProfileDraft => ({ name: "", description: "", engine: null, model_tier: "builder", permission_mode: "supervised", autopilot_mode: "off" });

/** "Name (2)" for a duplicate: the first free one among the existing names, compared without regard to case. */
export function duplicateName(base: string, existing: readonly string[]): string {
  const used = new Set(existing.map((name) => name.trim().toLowerCase()));
  const stem = plain(base, NAME_MAX - 5);
  for (let n = 2; n < 100; n += 1) {
    const candidate = `${stem} (${n})`;
    if (!used.has(candidate.toLowerCase())) return candidate;
  }
  return stem;
}

/** The second line of a row: engine, tier, permission mode and Autopilot, in the words of the frames. */
export function profileLine(profile: Profile, engineName: string): string {
  return [engineName, tierLabel(profile.model_tier), modeLabel(profile.permission_mode), `Autopilot ${autopilotLabel(profile.autopilot_mode)}`].join(" · ");
}
