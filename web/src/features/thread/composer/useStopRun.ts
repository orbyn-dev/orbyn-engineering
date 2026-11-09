import { useState } from "react";
import { api } from "../../../api/client";
import type { Thread } from "../../../api/types";
import { toFailure } from "../../../lib/useAsync";
import { useWorkspace } from "../../../product/state/workspace";

/** Stops the thread's active run and remembers which one, for the notice that follows. */
export function useStopRun(thread: Thread, setError: (message: string | null) => void) {
  const [stopping, setStopping] = useState(false);
  const [stoppedRun, setStoppedRun] = useState<string | null>(null);
  const refreshAll = useWorkspace((state) => state.refreshAll);

  const stop = async () => {
    if (!thread.active_run) return;
    setStopping(true);
    setError(null);
    try {
      await api.cancelRun(thread.active_run.id);
      setStoppedRun(thread.active_run.id);
      void refreshAll();
    } catch (failure) {
      setError(`Couldn't stop the run. ${toFailure(failure).message}`);
    } finally {
      setStopping(false);
    }
  };

  return { stopping, stoppedRun, stop };
}
