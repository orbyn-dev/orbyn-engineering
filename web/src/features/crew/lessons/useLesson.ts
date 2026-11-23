import { useCallback, useRef } from "react";
import type { ApiFailure } from "../../../api/client";
import { useAsync } from "../../../lib/useAsync";
import { lessonsApi } from "./api";
import type { Lesson, LessonDetail } from "./types";

/**
 * One lesson and its evidence. A 404 `not_found` is not an error: it is the answer of a service that has no lessons
 * routes (Crew Preview off, or an older service), and the card draws nothing at all then. Any other failure is the red
 * error state. A write's answer replaces the lesson in place, so the card draws the service's own new state.
 */
export type LessonRead =
  | { status: "loading"; detail: null; failure: null }
  | { status: "missing"; detail: null; failure: null }
  | { status: "error"; detail: null; failure: ApiFailure }
  | { status: "ready"; detail: LessonDetail; failure: null };

export type LessonReader = LessonRead & {
  reload(): Promise<void>;
  /** The whole answer, after a read or a write. */
  setDetail(detail: LessonDetail): void;
  /** A write's own answer: the lesson changes, its evidence stays. */
  setLesson(lesson: Lesson): void;
};

export function useLesson(repo: string, id: string): LessonReader {
  const read = useAsync(`crew-lesson:${repo}:${id}`, (signal) => lessonsApi.detail(id, repo, signal));
  const latest = useRef<LessonDetail | null>(null);
  latest.current = read.status === "ready" ? read.data : null;
  const { setData } = read;
  const setLesson = useCallback(
    (lesson: Lesson) => {
      const detail = latest.current;
      if (detail) setData({ ...detail, lesson });
    },
    [setData],
  );
  const state: LessonRead =
    read.status === "ready"
      ? { status: "ready", detail: read.data, failure: null }
      : read.status === "error"
        ? read.error.status === 404
          ? { status: "missing", detail: null, failure: null }
          : { status: "error", detail: null, failure: read.error }
        : { status: "loading", detail: null, failure: null };
  return Object.assign(state, { reload: read.reload, setDetail: setData, setLesson });
}
