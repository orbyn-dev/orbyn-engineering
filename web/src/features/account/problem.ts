import { NETWORK_ERROR } from "../../api/client";
import { toFailure } from "../../lib/useAsync";

/** What went wrong with an account request, in the few kinds the screens tell apart. */
export type Problem =
  | { kind: "wrong" }
  | { kind: "network" }
  | { kind: "throttled"; seconds: number }
  | { kind: "code"; triesLeft: number | null }
  | { kind: "closed" }
  | { kind: "invite" }
  /** The short check this device does before a code is sent could not finish. */
  | { kind: "check" }
  | { kind: "other"; message: string };

export const NETWORK_TEXT = "Couldn’t reach Orbyn. Check your connection and try again. Nothing was changed.";

export function problemOf(error: unknown): Problem {
  const failure = toFailure(error);
  const detail = (failure.detail ?? {}) as { retry_after_secs?: number; tries_left?: number };
  switch (failure.code) {
    case NETWORK_ERROR:
    case "account_offline":
    case "account_unavailable":
      return { kind: "network" };
    case "rate_limited":
      return { kind: "throttled", seconds: detail.retry_after_secs ?? 60 };
    case "wrong_password":
      return { kind: "wrong" };
    case "invalid_code":
      return { kind: "code", triesLeft: detail.tries_left ?? null };
    case "sign_in_closed":
      return { kind: "closed" };
    case "check_failed":
      return { kind: "check" };
    case "invite_required":
    case "invalid_invite":
      return { kind: "invite" };
    default:
      return { kind: "other", message: failure.message };
  }
}
