/**
 * The execution profile contract of docs/execution-profiles-and-library.md, Part 1 (W8), as the service answers it (routes in
 * crates/orbynd/src/routes/profiles.rs and crew.rs; the generated types in src/generated are the reference and are not edited).
 * `GET /api/profiles` is a bare array; `ProfileList` is the client's own view of it with the fixed limit of 50.
 */

import type { ApplyProfile } from "../../generated/ApplyProfile";
import type { ApplyProfileAnswer } from "../../generated/ApplyProfileAnswer";
import type { Profile } from "../../generated/Profile";
import type { ProfileCheck } from "../../generated/ProfileCheck";
import type { ProfileDraft } from "../../generated/ProfileDraft";
import type { ProfileGrant } from "../../generated/ProfileGrant";

// The wire types are the generated ones; nothing here repeats them. Only the client's own views are defined below.
export type { AutopilotMode } from "../../generated/AutopilotMode";
export type { PermissionMode } from "../../generated/PermissionMode";
export type { ApplyProfileAnswer, Profile, ProfileCheck, ProfileDraft, ProfileGrant };
export type { ProfileApplication } from "../../generated/ProfileApplication";
export type { ProfileAvailability } from "../../generated/ProfileAvailability";
export type { ProfileTier } from "../../generated/ProfileTier";
export type { ProfileWarning } from "../../generated/ProfileWarning";

/**
 * The body of an apply, for a thread or (with `crew_id` added by the client) a crew. `ack_grants` is optional here because the service
 * defaults it (`#[serde(default)]`), though the generated type lists it as required: the client sends it only for a grant-raising profile.
 */
export type ApplyBody = Omit<ApplyProfile, "ack_grants"> & { ack_grants?: ProfileGrant[] };

/** What the client hands its screens: the bare array the service answers, and the constant limit of 50 (not in the answer). */
export interface ProfileList {
  profiles: Profile[];
  limit: number;
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
  applyToThread(threadId: string, body: ApplyBody): Promise<ApplyProfileAnswer>;
  applyToCrew(crewId: string, body: ApplyBody & { profile_id: string }): Promise<ApplyProfileAnswer>;
}
