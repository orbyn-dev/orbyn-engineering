/**
 * What a thread on an engine that never asks before acting (Cursor) needs from the service, as the thread screens read it.
 *
 * PLANNED CONTRACT. The Rust side is HostDriver slice 3 (output/harness-crew/cursor-thread-contract-eng2.md) and is not merged:
 * no type here is generated. Every shape below is a guess from that note until the routes land; the PR lists each one. When they
 * land, replace these hand-written types with the generated ones in the same commit that swaps the client in `api.ts`.
 *
 * Nothing here can carry a secret: a notice is a flag and a date, and a refusal is a sentence and a code.
 */
import type { PermissionMode } from "../../api/types";

/** The run-start refusals the service answers with HTTP 409. The screens act on the `code`, never on the message. */
export const MODE_REFUSED = "engine_mode_refused";
export const NOTICE_REQUIRED = "engine_notice_required";

/**
 * The one mode an engine that never asks offers. GUESS: the wire spelling is confirmed only in the service's own PR; the generated
 * `PermissionMode` does not name it yet, so the engine's list is read as strings.
 */
export const NO_APPROVALS = "no_approvals";

/** `GET /api/engines/{engine}/notice`, and what `POST .../notice/acknowledge` answers. One local setting per engine. */
export interface NoticeState {
  engine: string;
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

/** `EngineStatus.crew_role` and `crew_role_reason`, added by the service slice; absent on a service without it. */
export interface CrewRoleFields {
  crew_role?: "any" | "reviewer_only";
  crew_role_reason?: string | null;
}

/** The routes the web does not yet have generated clients for. */
export interface CursorThreadApi {
  /** `null` when this service has no notice for the engine (every engine but one, or a service without the route). */
  notice(engine: string, signal?: AbortSignal): Promise<NoticeState | null>;
  /** One click, one write. Person-class: the key goes with it. */
  acknowledge(engine: string): Promise<NoticeState>;
  /**
   * Runs ONE turn with No approvals and stores nothing: the thread's saved mode stays. Person-class, never remembered. The body is
   * the same as a send. Nothing in the screens calls it until a frame draws the control that does.
   */
  sendWithoutApprovals(threadId: string, body: { prompt: string; attachment_ids?: string[] }): Promise<unknown>;
}
