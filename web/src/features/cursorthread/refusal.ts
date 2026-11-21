import { ApiFailure } from "../../api/client";
import { MODE_REFUSED, NOTICE_REQUIRED } from "./contract";

/** A sentence from the service is shown as plain text only (React escapes it) and never longer than a line of a card. */
const LIMIT = 280;

export type RunRefusal =
  | { kind: "mode"; message: string | null }
  | { kind: "notice"; message: null };

/** The service's own words, tidied: one run of text, bounded, or null when it sent none. */
export function displayText(message: unknown): string | null {
  if (typeof message !== "string") return null;
  const text = message.replace(/\s+/g, " ").trim();
  if (text === "") return null;
  return text.length > LIMIT ? `${text.slice(0, LIMIT - 1)}…` : text;
}

/** Which refusal a failed run start is, by its `code` and a 409 only; anything else is not ours. */
export function runRefusal(failure: unknown): RunRefusal | null {
  if (!(failure instanceof ApiFailure) || failure.status !== 409) return null;
  if (failure.code === MODE_REFUSED) return { kind: "mode", message: displayText(failure.message) };
  if (failure.code === NOTICE_REQUIRED) return { kind: "notice", message: null };
  return null;
}
