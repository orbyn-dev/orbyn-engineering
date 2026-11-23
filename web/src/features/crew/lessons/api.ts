// The lessons client (KC5): reads are Local class and Preview-gated, so a page, its retry and its filter may all
// send them. The person's writes are Person-only and are never sent by a load, a refresh or a retry of a read:
// only a person's own click may add, edit, disable, enable, delete, undo, promote, demote, or flip the switch.
// A service without the lessons routes answers 404 not_found to every read, and the screens draw nothing then.
import { ApiFailure, request } from "../../../api/client";
import type { Lesson, LessonDeleted, LessonDetail, LessonFilter, LessonList, LessonScope } from "./types";

/** The same shape the service's every crew id check accepts. */
const validLessonId = (id: string): boolean => id.length <= 96 && /^[A-Za-z0-9:_-][A-Za-z0-9:_.-]*$/.test(id);

const repoQuery = (repo: string) => `repo=${encodeURIComponent(repo)}`;
const path = (id: string): string => {
  if (!validLessonId(id)) throw new ApiFailure("invalid", "That lesson’s id is not valid.", 0);
  return `/api/crew/lessons/${encodeURIComponent(id)}`;
};

/** The body of a hand-written lesson: `repo` is required when the scope is a repository, absent for all of them. */
export interface LessonAdd {
  scope: LessonScope;
  repo?: string;
  text: string;
}

/** A write's own change: the text the person read, and the revision it started from. */
export interface LessonEdit {
  text: string;
  rev: number;
}

/** The answer of the per-repository switch. */
export interface LessonUse {
  repo: string;
  use_lessons: boolean;
}

export const lessonsApi = {
  /** One repository's lessons under one filter, and its switch, counts and brief cap. A read, never a write. */
  list: (repo: string, filter: LessonFilter, signal?: AbortSignal) =>
    request<LessonList>("GET", `/api/crew/lessons?${repoQuery(repo)}&filter=${filter}`, undefined, signal),
  /** One lesson and its evidence. `repo` lets the service say whether a global lesson would go into that repository's next brief. */
  detail: (id: string, repo: string, signal?: AbortSignal) =>
    request<LessonDetail>("GET", `${path(id)}?${repoQuery(repo)}`, undefined, signal),
  /** A person adds a lesson by hand. Their own click sends it; the answer is the lesson as written. */
  add: (body: LessonAdd) => request<Lesson>("POST", "/api/crew/lessons", body),
  /** A person edits a lesson's text. Their own click sends it; the service sets `edited_by_person` and re-runs the scan. */
  edit: (id: string, body: LessonEdit) => request<Lesson>("PATCH", path(id), body),
  /** A person turns a lesson off. It stays readable and never reaches a brief. */
  disable: (id: string, rev: number) => request<Lesson>("POST", `${path(id)}/disable`, { rev }),
  /** A person turns a lesson back on. It may be over the cap then, so the read says so. */
  enable: (id: string, rev: number) => request<Lesson>("POST", `${path(id)}/enable`, { rev }),
  /** A person deletes a lesson: undoable for ten seconds, final after that. */
  remove: (id: string, rev: number) =>
    request<LessonDeleted>("DELETE", `${path(id)}?rev=${encodeURIComponent(String(rev))}`),
  /** A person takes back their own delete, while its window is open. */
  undo: (id: string) => request<Lesson>("POST", `${path(id)}/undo`),
  /** A person promotes a repository lesson to all repositories. Only a person may; nothing promotes by itself. */
  promote: (id: string, rev: number) => request<Lesson>("POST", `${path(id)}/promote`, { rev }),
  /** A person moves an all-repositories lesson back to one repository. */
  demote: (id: string, body: { repo: string; rev: number }) => request<Lesson>("POST", `${path(id)}/demote`, body),
  /** A person turns the per-repository switch on or off. Their own click sends it, on a change they chose. */
  setUse: (body: LessonUse) => request<LessonUse>("PUT", "/api/crew/lessons/settings", body),
};
