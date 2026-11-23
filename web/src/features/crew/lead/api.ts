// The lead thread's live Crew work: what settling or archiving the thread would cancel, and the close itself. Both routes are
// local and Crew Preview only; a 404 means the service has no such feature yet, so the thread settles exactly as it did before.
import { ApiFailure, request } from "../../../api/client";

export interface LiveTask {
  id: string;
  name: string;
  /** A Flow state word, such as `routed`, `running` or `needs_person`. */
  state: string;
}

interface LiveAnswer {
  tasks: LiveTask[];
}

export interface CloseAnswer {
  closed: string[];
  failed: string[];
}

const path = (threadId: string, what: "live" | "close") =>
  `/api/crew/lead/${encodeURIComponent(threadId)}/${what}`;

export const leadApi = {
  /** The tasks a close would cancel, in order. `null` when the route is missing (404): the caller keeps its old path. */
  async live(threadId: string, signal?: AbortSignal): Promise<LiveTask[] | null> {
    try {
      const answer = await request<LiveAnswer>("GET", path(threadId, "live"), undefined, signal);
      return answer.tasks;
    } catch (failure) {
      if (failure instanceof ApiFailure && failure.status === 404) return null;
      throw failure;
    }
  },
  /** Tries every live task once and answers ids only. The thread settles whatever the answer says. */
  close: (threadId: string): Promise<CloseAnswer> => request<CloseAnswer>("POST", path(threadId, "close")),
};

// The service enforces this close as a person route: `["POST", "/api/crew/lead/{thread_id}/close"]` is in PERSON_ROUTES in
// web/src/api/personKey.ts, so the page sends the key for it.
