/**
 * The execution profile contract of docs/execution-profiles-and-library.md, Part 1 (W8), as the service answers it (routes in
 * crates/orbynd/src/routes/profiles.rs and crew.rs; the generated types in src/generated are the reference and are not edited).
 * `GET /api/profiles` is a bare array; `ProfileList` is the client's own view of it with the fixed limit of 50.
 */

export type ModelTier = "lead" | "builder" | "scout";
export type PermissionModeId = "supervised" | "auto_edits" | "auto" | "full_access";
export type AutopilotModeId = "off" | "on" | "full";
/** The grant codes of "Applying a grant-raising profile". */
export type GrantCode = "full_access" | "autopilot_full";

export interface Availability {
  /** A word the web does not know is shown as Unknown, never as a green light. */
  engine: "ok" | "not_found" | "cannot_start" | "disabled" | "unmanaged" | "none";
  reason?: string;
}

export interface ProfileWarning {
  field: string;
  code: string;
  message: string;
}

export interface Profile {
  id: string;
  name: string;
  description: string;
  /** An engine id, or `null`: keep the thread's engine. */
  engine: string | null;
  model_tier: ModelTier;
  permission_mode: PermissionModeId;
  autopilot_mode: AutopilotModeId;
  /** Read-only, listed first, cannot be edited or deleted. */
  builtin: boolean;
  revision: number;
  created_at: string;
  updated_at: string;
  availability?: Availability;
  warnings?: ProfileWarning[];
}

/** The body of a create or a check: the shape minus `id`, `builtin`, `revision` and timestamps. */
export interface ProfileDraft {
  name: string;
  description: string;
  engine: string | null;
  model_tier: ModelTier;
  permission_mode: PermissionModeId;
  autopilot_mode: AutopilotModeId;
}

/** What the client hands its screens: the bare array the service answers, and the constant limit of 50 (not in the answer). */
export interface ProfileList {
  profiles: Profile[];
  limit: number;
}

export interface ProfileCheck {
  warnings: ProfileWarning[];
  availability: Availability;
}

/** `PUT /api/threads/{id}/profile` (and, with `crew_id`, `POST /api/crew/profile/apply`). */
export interface ApplyBody {
  /** `null` on a thread only forgets the chip and keeps the values. */
  profile_id: string | null;
  seen_revision?: number;
  /** Only for a grant-raising profile: the codes the person saw. */
  ack_grants?: GrantCode[];
}

export interface ApplyAnswer {
  applied: { engine?: string; model?: string; permission_mode?: PermissionModeId; /** A crew apply only. */ autopilot_mode?: AutopilotModeId };
  /** Fields that do not apply to this target, such as `autopilot_mode` on a plain thread. */
  ignored: string[];
  /** `null` when the chip was cleared (`profile_id: null`): the answer is `null` and carries no application row. */
  application: null | { profile_id: string; name: string; revision: number; customized: boolean; applied_at: string };
}

export const NAME_MAX = 60;
export const DESCRIPTION_MAX = 200;
export const PROFILE_LIMIT = 50;

export interface ProfilesApi {
  /** `null` when the service has no profiles route (`404`): the feature is hidden, with no error. */
  list(signal?: AbortSignal): Promise<ProfileList | null>;
  /** Saves nothing; used by the editor while typing. */
  check(draft: ProfileDraft, signal?: AbortSignal): Promise<ProfileCheck>;
  /** Writes. Person-class (`PENDING_PERSON_ROUTES`). */
  create(draft: ProfileDraft): Promise<Profile>;
  update(id: string, draft: ProfileDraft, seenRevision: number): Promise<Profile>;
  remove(id: string): Promise<void>;
  applyToThread(threadId: string, body: ApplyBody): Promise<ApplyAnswer>;
  applyToCrew(crewId: string, body: ApplyBody & { profile_id: string }): Promise<ApplyAnswer>;
}
