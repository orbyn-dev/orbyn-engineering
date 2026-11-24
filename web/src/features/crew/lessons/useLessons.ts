import type { ApiFailure } from "../../../api/client";
import { useAsync } from "../../../lib/useAsync";
import { lessonsApi } from "./api";
import type { LessonFilter, LessonList } from "./types";

/**
 * The read of one repository's lessons under one filter. A 404 `not_found` is not an error: it is the answer of a
 * service that has no lessons routes (Crew Preview off, or an older service), and the page draws nothing at all.
 * Any other failure is the red error state. The read is aborted when the repository or the filter changes, and
 * `reload()` reads the same one again.
 */
export type LessonsRead =
  | { status: "loading"; list: null; failure: null }
  | { status: "missing"; list: null; failure: null }
  | { status: "error"; list: null; failure: ApiFailure }
  | { status: "ready"; list: LessonList; failure: null };

export type LessonsReader = LessonsRead & {
  reload(): Promise<void>;
  /** Replaces the read's list after a write answered, under the key it was read with. */
  setList(list: LessonList): void;
};

/** The reader once its read answered 200. */
export type LessonsReady = Extract<LessonsRead, { status: "ready" }> & {
  reload(): Promise<void>;
  setList(list: LessonList): void;
};

export function useLessons(repo: string, filter: LessonFilter): LessonsReader {
  const read = useAsync(`crew-lessons:${repo}:${filter}`, (signal) => lessonsApi.list(repo, filter, signal));
  const state: LessonsRead =
    read.status === "ready"
      ? { status: "ready", list: read.data, failure: null }
      : read.status === "error"
        ? read.error.status === 404
          ? { status: "missing", list: null, failure: null }
          : { status: "error", list: null, failure: read.error }
        : { status: "loading", list: null, failure: null };
  return Object.assign(state, { reload: read.reload, setList: read.setData });
}
