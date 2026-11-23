/**
 * The lessons wire types, written by hand from docs/crew-lessons.md (KC5, KC9). There is no generated type for
 * them yet. They move to web/src/generated when core slice 3 lands, and this file goes away then: until that
 * slice, every shape here is the contract's own.
 */

/** One short plain sentence Crew keeps about one repository, or about all of them. */
export type LessonScope = "repo" | "global";

/** Where a lesson came from: a repeat of one failing check, a confirmed escape, a confirmed revert, or a person's own correction. */
export type LessonSource = "fingerprint" | "escape" | "revert" | "person";

/** The list's own filter. `hidden` is the lessons over the cap, still readable but never in a brief. */
export type LessonFilter = "active" | "disabled" | "hidden" | "all";

/** A lesson as the service answers it. It is data in a brief, never a rule. */
export interface Lesson {
  id: string;
  scope: LessonScope;
  /** The repository id for a repository lesson; null for an all-repositories lesson. */
  repo: string | null;
  source: LessonSource;
  /** 1 to 300 characters. */
  text: string;
  state: "active" | "disabled";
  /** Over the cap (50 per repository, 20 global): readable under the Hidden filter, never in a brief. */
  hidden_by_cap: boolean;
  /** Would go into the next brief for the repository the read names: active, in the newest 5 and at most 1500 characters, switch on, no skill. */
  in_next_brief: boolean;
  edited_by_person: boolean;
  /** The text names a repository path (repository scope only); promoting such a lesson is refused until it is edited. */
  names_path: boolean;
  /** Briefs that carried it. */
  uses: number;
  /** Briefs it was left out of because a check it matches was in the task. */
  left_out_of: number;
  /** KC9: the skill made from it, while one exists. A lesson with a skill is not in a brief. */
  skill: { name: string } | null;
  created_at: string;
  updated_at: string;
  last_used_at: string | null;
  /** Integer, +1 on every change; every write sends it back. */
  rev: number;
}

/** A lesson's evidence: ids only, never content. */
export interface Evidence {
  kind: "task" | "escape" | "merge";
  id: string;
}

/** GET /api/crew/lessons: one repository's list, its switch, the brief cap and the counts. */
export interface LessonList {
  repo: string;
  use_lessons: boolean;
  brief_cap: { lessons: number; chars: number };
  counts: { active: number; disabled: number; hidden: number };
  lessons: Lesson[];
}

/** GET /api/crew/lessons/{id}: one lesson and its evidence. */
export interface LessonDetail {
  lesson: Lesson;
  evidence: Evidence[];
}

/** DELETE /api/crew/lessons/{id}: the deleted id and the end of its undo window. */
export interface LessonDeleted {
  id: string;
  undo_until: string;
}
