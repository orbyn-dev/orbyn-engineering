import { dayOf } from "../../autopilot/approvals/ledger";
import type { Lesson } from "../types";
import type { SkillProposal } from "./types";

/** The device-local area an approved skill is written to (Eng 4's ruling); the web never writes it itself. */
export const SKILLS_LOCAL = "skills-local";
export const ALL_PROJECTS = "All your projects";
export const DEFAULT_SELECTION_NOTE = "joins the Default selection";

/** The sentence every approved place repeats, as Engineering 4 ruled it. */
export const AVAILABLE =
  "Available to threads on the Default profile through Orbyn's skills tools. Applies to all your projects on this device.";

/** The group note on the Skills page. */
export const ADDED_GROUP_NOTE =
  "Available to threads on the Default profile through Orbyn's skills tools. Applies to all your projects on this device. Saved in skills-local on this device, not in your repository.";

/** The one line the Skills page draws above Built in while nothing has been added from lessons. */
export const ADDED_EMPTY_LINE =
  "Nothing added from lessons yet. A skill you approve from a Crew proposal is listed here, above Built in, as Personal.";

/** The sentence under the Proposed card on the Skills page. */
export const PROPOSED_GROUP_NOTE =
  "Crew proposed this from 3 or more active lessons on this device. Nothing is added until you review and approve it; Review opens the same page as on Lessons. Once added it is listed under “Added from lessons” as Personal. Available to threads on the Default profile through Orbyn's skills tools. Applies to all your projects on this device.";

/** The card's three lines under the text preview. */
export const NOTHING_ADDED =
  "Nothing is added until you approve. A lesson is data in a brief; a skill is instructions, so you read it first. Until then skills-local is unchanged.";
export const TEMPLATE_NOTE =
  "The text is filled in from a fixed template, not written by a model, and you can edit it before you approve.";
export const SCAN_NOTE =
  "Crew is meant to scan the text for secrets before it makes a proposal, so a text that holds one is never proposed, and to scan it again when you edit it.";

/** The review page's banner and the sentence under its box. */
export const REVIEW_BANNER =
  "Nothing is installed or enabled until you press Approve. Until then skills-local is unchanged: the skill is not listed and not used.";
export const REVIEW_NOTE =
  "The text is filled in from a fixed template, not written by a model. You can edit it before you approve. Crew is meant to scan the text for secrets before it makes a proposal, so a text that holds one is never proposed, and to scan it again when you edit it.";
/** The same note once the write is out: nothing is editable then, so the "you can edit it" sentence is gone. */
export const REVIEW_NOTE_BUSY =
  "The text was filled in from a fixed template, not written by a model. Crew is meant to scan the text for secrets before it makes a proposal, so a text that holds one is never proposed, and to scan it again when you edit it.";
export const REVIEW_FOOT =
  "Approve saves the skill in skills-local on this device and lists it on the Skills page as Personal. Available to threads on the Default profile through Orbyn's skills tools. Applies to all your projects on this device. Cancel leaves the proposal where it is.";
export const APPROVING_BANNER = "Adding the skill. It is not listed or used until the write has finished.";
/** The review page's sentence while the repository's lessons have not been read yet, so the active count is unknown. */
export const STALE_APPROVE_OFF_WAITING =
  "This proposal is stale: a lesson it was based on was deleted or turned off. Go back to dismiss it.";
/** The refused-write frame's sentence: that frame offers Try again, so it is drawn on a failed write and not on the scan refusal. */
export const REFUSED_FOOT = "Nothing was written. Approve or Try again sends the same text once more.";
/** The footer when the service did not answer: the write may or may not have been made, so "nothing was written" is not said. */
export const UNKNOWN_FOOT = "It is not known whether the skill was written. Try again sends the same text once more if the proposal is still open.";

/** The dismiss confirm's questions. */
export const DISMISS_NOTE = "Nothing was added to skills-local. The lessons it was based on stay as they are.";

/** The toast after a dismissal. The design draws “Crew won’t propose this pattern again” and an Undo; the contract carries
 *  neither a mute nor an undo route, so the web says only what it did. */
export const DISMISSED_TOAST = "Proposal dismissed.";

/** A moment as the design draws it: "Sat 7 Nov, 16:31". */
export function moment(iso: string): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return "";
  const clock = `${String(at.getHours()).padStart(2, "0")}:${String(at.getMinutes()).padStart(2, "0")}`;
  return `${dayOf(iso)}, ${clock}`;
}

/** A count of lessons, in the words the card and the dialogs use: "3 lessons". */
export const lessonsWord = (n: number): string => `${n} ${n === 1 ? "lesson" : "lessons"}`;

/** The word alone, where a count already precedes it: "3 active lessons". */
export const lessonsOnly = (n: number): string => (n === 1 ? "lesson" : "lessons");

/** What a SKILL.md text says about itself, read the way the design draws it: its name and one line. */
export interface SkillText {
  name: string | null;
  description: string | null;
  /** How many lines the whole text has, for "first 8 of 14 lines" and the count under the review box. */
  lines: number;
}

const unquote = (value: string): string => value.trim().replace(/^["']|["']$/g, "").trim();

/**
 * A proposal's SKILL.md text as its front matter states it: the name and the one line between the two `---` fences. A text
 * without that front matter has neither, and the screens leave those rows out rather than guessing.
 */
export function skillText(text: string): SkillText {
  const lines = text.split("\n");
  const rendered = { name: null as string | null, description: null as string | null, lines: lines.length };
  if (lines[0]?.trim() !== "---") return rendered;
  const end = lines.findIndex((line, at) => at > 0 && line.trim() === "---");
  if (end < 0) return rendered;
  for (const line of lines.slice(1, end)) {
    const name = /^name:\s*(.+)$/.exec(line);
    if (name) rendered.name = unquote(name[1] ?? "") || null;
    const description = /^description:\s*(.+)$/.exec(line);
    if (description) rendered.description = unquote(description[1] ?? "") || null;
  }
  return rendered;
}

/** The first `count` lines a card previews, with the ellipsis the design draws when the text is longer. */
export function firstLines(text: string, count: number): string {
  const lines = text.split("\n");
  return lines.slice(0, count).join("\n") + (lines.length > count ? "\n…" : "");
}

/** How many of a proposal's source lessons are active now: a lesson that was deleted or turned off is not. */
export function activeLessons(proposal: SkillProposal, lessons: readonly Lesson[]): number {
  const byId = new Map(lessons.map((lesson) => [lesson.id, lesson]));
  return proposal.lessons.filter((one) => byId.get(one.id)?.state === "active").length;
}

/** The source lessons a read of the repository's lessons still holds, in the proposal's own order. */
export function sourceLessons(proposal: SkillProposal, lessons: readonly Lesson[]): Lesson[] {
  const byId = new Map(lessons.map((lesson) => [lesson.id, lesson]));
  return proposal.lessons.flatMap((one) => (byId.has(one.id) ? [byId.get(one.id) as Lesson] : []));
}

/** "3 active lessons with the same source, check kind and path class": the contract's own rule for a proposal. */
export function basedOnWords(proposal: SkillProposal, lessons: readonly Lesson[]): string {
  const active = activeLessons(proposal, lessons);
  return proposal.can_approve
    ? `${proposal.lessons.length} active ${lessonsOnly(proposal.lessons.length)} with the same source, check kind and path class`
    : `${active} active ${lessonsOnly(active)} now`;
}

/** The amber line of a proposal whose pattern lost a lesson: the count now, and that Approve is off. */
export function cardStaleWords(proposal: SkillProposal, lessons: readonly Lesson[]): string {
  const active = activeLessons(proposal, lessons);
  const rest = "You can still read the text or dismiss it; Approve is off.";
  // The contract turns Approve off when any source lesson is gone, so three or more can remain: say what is true then.
  if (active >= 3)
    return `Based on ${active} active ${lessonsOnly(active)} now, ${proposal.lessons.length} when it was proposed. A lesson it was based on was deleted or turned off, so the text no longer matches its lessons. ${rest}`;
  return `Based on ${active} active ${lessonsOnly(active)} now: needs 3. A lesson it was based on was deleted or turned off, so the pattern no longer has three active ones. ${rest}`;
}

/** The review page's sentence once the repository's lessons say how many of the proposal's lessons remain active. */
export function staleWords(active: number): string {
  return `This proposal is stale: it now rests on ${active} ${active === 1 ? "lesson" : "lessons"} and needs 3. Go back to dismiss it.`;
}

/** The name a proposal's text gives, falling back to the row's own word. */
export const proposalName = (proposal: SkillProposal): string => skillText(proposal.text).name ?? "Proposed skill";
