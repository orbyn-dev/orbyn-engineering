import type { RunState, Thread } from "../../../api/types";

/** The server's current state for a thread. The UI derives nothing further. */
export const threadRunState = (thread: Thread): RunState | null =>
  thread.active_run?.state ?? thread.last_run_state;
