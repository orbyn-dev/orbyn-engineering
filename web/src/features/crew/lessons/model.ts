import type { Lesson, LessonEvidence, LessonScope, LessonSource } from "./types";
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

/** Whether a read named a source these tables know; a newer service may name another one. */
const knownSource = (source: string): source is LessonSource =>
  Object.prototype.hasOwnProperty.call(SOURCE_WORD, source);

/** The word for a source: the design's own, or the raw string for one a newer service named. */
export const sourceWord = (source: string): string => (knownSource(source) ? SOURCE_WORD[source] : source);

/** The chip class for a source: the design's own tone, or the plain chip for one a newer service named. */
export const sourceChip = (source: string): string => (knownSource(source) ? SOURCE_CHIP[source] : "cx-apchip");

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

/** One word with its first letter capitalised, for a kind the contract does not name. */
const capitalised = (word: string): string => word.charAt(0).toUpperCase() + word.slice(1);

/**
 * One evidence id with the word its kind carries: "Task 5b2d8a3", "Escape 9be4410", "Revert 3b9f2c1"; a kind a newer
 * service names is drawn by its own capitalised word.
 */
export function evidenceWord(one: LessonEvidence): string {
  const kind =
    one.kind === "task"
      ? "Task"
      : one.kind === "escape"
        ? "Escape"
        : one.kind === "merge" || one.kind === "revert"
          ? "Revert"
          : capitalised(one.kind);
  return `${kind} ${one.id}`;
}

/** The evidence ids a lesson came from, as one line; empty when there are none. */
export const evidenceLine = (evidence: readonly LessonEvidence[]): string => evidence.map(evidenceWord).join(", ");

/** The word for a lesson's source: a person's lesson with no evidence behind it was added by hand. */
export function sourceWordOf(source: string, evidence: readonly LessonEvidence[]): string {
  return source === "person" && evidence.length === 0 ? "Added by you" : sourceWord(source);
}

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

/**
 * What a lesson is, for its own card: the contract's source, its evidence ids only (never content),
 * and where it applies. The design draws a fingerprint line and the names behind a repeat, and a
 * correction's own action ("your answer", "your override", a finding edited); the read carries
 * neither, so those rows stay out rather than being made up here.
 */
export function happenedWords(lesson: Lesson): string {
  return lesson.source === "fingerprint"
    ? "The same failing check, ended the same way, with the same names of what failed, on 2 or more tasks of this repository within 60 days. The text is built from names Crew already redacts for a worker, never from worker output."
    : lesson.source === "escape"
      ? `You confirmed this escape on ${moment(lesson.created_at)}. The text is the escape’s own recorded reason.`
      : lesson.source === "revert"
        ? `You reverted this merge on ${moment(lesson.created_at)}. The text is the reason that was recorded with the revert. A revert you make is a source; a revert Crew makes itself writes no lesson until you confirm it as an escape.`
        : "The lesson is the words you typed, capped, and it is marked as yours.";
}

/** The sentence the secret scan's refusal carries, exactly as the contract fixes it. */
export const SECRET_REFUSED = "Not saved: this looks like it holds a secret. Edit it and save again.";

/** The drawn line under a card or a form: whose permissions a worker runs with. */
export const PERMISSIONS_NOTE = "Workers run as you, with your machine’s permissions.";
