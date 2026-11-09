import { api } from "../../../api/client";
import type { PermissionMode, Thread } from "../../../api/types";
import { toFailure } from "../../../lib/useAsync";
import { useUi } from "../../../state/ui";
import { useWorkspace } from "../../../state/workspace";

/** Changes to the thread's mode, access, effort and model, each reporting its own failure. */
export function useThreadSettings(thread: Thread, setError: (message: string | null) => void) {
  const putThread = useWorkspace((state) => state.putThread);
  const setPrefs = useUi((state) => state.setPrefs);
  const setPlanMode = async (on: boolean) => {
    setError(null);
    try {
      putThread(await api.patchThread(thread.id, { plan_mode: on }));
    } catch (failure) {
      setError(`Couldn't change the mode. ${toFailure(failure).message}`);
    }
  };
  const setAccess = async (mode: PermissionMode) => {
    setError(null);
    try {
      putThread(await api.patchThread(thread.id, { permission_mode: mode }));
    } catch (failure) {
      setError(`Couldn't change access. ${toFailure(failure).message}`);
    }
  };
  const setEffort = async (effort: string) => {
    setError(null);
    try {
      putThread(await api.patchThread(thread.id, { effort }));
      setPrefs({ lastModel: { engine: thread.engine, model: thread.model, effort } });
    } catch (failure) {
      setError(`Couldn't change the effort. ${toFailure(failure).message}`);
    }
  };
  const pickModel = async (next: { engine: string; model: string | null }) => {
    setError(null);
    try {
      const updated =
        next.engine === thread.engine
          ? await api.patchThread(thread.id, next.model ? { model: next.model } : {})
          : await api.switchEngine(thread.id, next.engine, next.model);
      putThread(updated);
      setPrefs({ lastModel: { ...next, effort: updated.effort ?? null } });
    } catch (failure) {
      setError(`Couldn't change the model. ${toFailure(failure).message}`);
    }
  };
  return { putThread, setPlanMode, setAccess, setEffort, pickModel };
}
