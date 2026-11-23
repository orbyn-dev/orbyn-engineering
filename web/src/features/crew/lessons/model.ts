import type { Evidence, Lesson, LessonScope, LessonSource } from "./types";
import { dayOf } from "../autopilot/approvals/ledger";

/** A lesson's own words for its source, as the design's chip draws them. */
export const SOURCE_WORD: Record<LessonSource, string> = {
  fingerprint: "Repeated failure",
  escape: "Confirmed escape",
  revert: "Revert",
  person: "Your correction",
};

/** The chip's own tone: a confirmed escape is outlined, a person's correction quiet, the rest plain. */
export const SOURCE_CHIP: Record<LessonSource, string> = {
  fingerprint: "cx-apchip",
  escape: "cx-apchip cx-no",
  revert: "cx-apchip",
  person: "cx-apchip cx-dim",
};

export const SCOPE_WORD: Record<LessonScope, string> = { repo: "This repository", global: "All repositories" };

/** What a global lesson says about where it applies, next to its chip. */
export const EVERY_REPOSITORY = "every repository on this device";

/** The contract's one limit on a lesson's text. */
export const MAX_TEXT = 300;

/** The contract fixes the brief at most 5 lessons; a lesson's card is read without the list's own cap. */
export const BRIEF_LESSONS = 5;

const briefs = (n: number) => `${n} ${n === 1 ? "brief" : "briefs"}`;

/** How many briefs a lesson was left out of, in the design's words. */
export const leftOutWords = (n: number): string => `${briefs(n)}: it matched that task’s checks.`;

/** A count of briefs as the card says it: "1 brief" or "3 briefs". */
export const briefsWord = (n: number): string => briefs(n);

/** One evidence id with the word its kind carries: "Task 5b2d8a3", "Escape 9be4410", "Revert 3b9f2c1". */
export function evidenceWord(one: Evidence): string {
  const kind = one.kind === "task" ? "Task" : one.kind === "escape" ? "Escape" : "Revert";
  return `${kind} ${one.id}`;
}

/** The evidence ids a lesson came from, as one line; empty when there are none. */
export const evidenceLine = (evidence: readonly Evidence[]): string => evidence.map(evidenceWord).join(", ");

const clock = (at: Date) => `${String(at.getHours()).padStart(2, "0")}:${String(at.getMinutes()).padStart(2, "0")}`;

/** A moment as the design draws it: "Sat 7 Nov, 14:20". */
export function moment(iso: string): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return "";
  return `${dayOf(iso)}, ${clock(at)}`;
}

/** What a lesson reached: its briefs and the last one, or nothing yet. */
export function usedWords(lesson: Lesson): string {
  return lesson.last_used_at
    ? `Used in ${briefs(lesson.uses)} · last ${moment(lesson.last_used_at)}`
    : "Not used yet";
}

/** "Not edited", or the moment the person's own edit was saved. */
export function editedWords(lesson: Lesson): string {
  return lesson.edited_by_person ? `You, ${moment(lesson.updated_at)}` : "Not edited";
}

/**
 * What a lesson is, for its own card: the contract's source, its evidence ids only (never content),
 * and where it applies. The design draws a fingerprint line and the names behind a repeat, and a
 * correction's own action ("your answer", "your override", a finding edited); the read carries
 * neither, so those rows stay out rather than being made up here.
 */
export function happenedWords(source: LessonSource): string {
  return source === "fingerprint"
    ? "The same failing check, ended the same way, with the same names of what failed, on 2 tasks of this repository within 60 days. The text is built from names Crew already redacts for a worker, never from worker output."
    : source === "escape"
      ? "You confirmed this escape. The text is the escape’s own recorded reason."
      : source === "revert"
        ? "The text is the reason that was recorded with the revert. A revert you make is a source; a revert Crew makes itself writes no lesson until you confirm it as an escape."
        : "The lesson is the words you typed, capped, and it is marked as yours.";
}

/** The sentence the secret scan's refusal carries, exactly as the contract fixes it. */
export const SECRET_REFUSED = "Not saved: this looks like it holds a secret. Edit it and save again.";

/** The drawn line under a card or a form: whose permissions a worker runs with. */
export const PERMISSIONS_NOTE = "Workers run as you, with your machine’s permissions.";
