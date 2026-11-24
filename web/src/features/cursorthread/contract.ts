/**
 * What a thread on an engine that never asks before acting (Cursor) needs from the service, as the thread screens read it.
 *
 * Mirrors docs/cursor-thread-contract.md. The Rust side is HostDriver slice 3 and is not on main: no type here is generated and the
 * routes are not in `routes.json`. When they land, replace these hand-written types with the generated ones in the same commit that
 * swaps the client in `api.ts` and moves the two person routes into `PERSON_ROUTES`.
 *
 * Nothing here can carry a secret: a notice is a flag and a date, and a refusal is a sentence and a code.
 */
import type { PermissionMode } from "../../api/types";

/** The run-start refusals the service answers with HTTP 409. The screens act on the `code`, never on the message. */
export const MODE_REFUSED = "engine_mode_refused";
export const NOTICE_REQUIRED = "engine_notice_required";

/**
 * The one mode an engine that never asks offers: `EngineCapabilities.permission_modes` is `["no_approvals"]` for it. The generated
 * `PermissionMode` does not name it yet, so the engine's list is read as strings.
 */
export const NO_APPROVALS = "no_approvals";

/**
 * `GET /api/projects/{project_id}/engines/{engine}/notice`, and what `POST .../notice/acknowledge` answers. One local setting per
 * project and engine, not per person.
 */
export interface NoticeState {
  engine: string;
  project_id: string;
  acknowledged: boolean;
  acknowledged_at: string | null;
}

/** The body of an `engine_mode_refused` answer, beyond `code` and `message` (the web client keeps only those two). */
export interface ModeRefusal {
  engine: string;
  thread_id: string;
  mode: Extract<PermissionMode, "supervised" | "auto_edits">;
  allowed: string[];
}

/** The extra fields of an `engine_notice_required` answer (the web client keeps only `code` and `message`). */
export interface NoticeRefusal {
  engine: string;
  project_id: string;
}

/** `EngineStatus.crew_role` and `crew_role_reason`, added by the service slice; absent on a service without it. */
export interface CrewRoleFields {
  crew_role?: "any" | "reviewer_only";
  crew_role_reason?: string | null;
}

/** The routes the web does not yet have generated clients for. */
export interface CursorThreadApi {
  /** `null` when this service has no notice for the engine (404: every engine but one, or a service without the route). */
  notice(projectId: string, engine: string, signal?: AbortSignal): Promise<NoticeState | null>;
  /** One click, one write for the project. Person-class: the key goes with it. Acknowledging twice changes nothing. */
  acknowledge(projectId: string, engine: string): Promise<NoticeState>;
  /**
   * Runs ONE turn with No approvals and stores nothing: the thread's saved mode stays. Person-class, never remembered. The body is
   * the same as a send. It does not skip the notice: an unacknowledged project still answers `engine_notice_required`.
   */
  sendWithoutApprovals(threadId: string, body: { prompt: string; attachment_ids?: string[] }): Promise<unknown>;
}
