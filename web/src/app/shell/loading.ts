import { useWorkspace } from "../../product/state/workspace";

export const HIDDEN_POLL_EVERY = 3;


// One load at a time: a remount (React's development double mount included) or a
// slow load meeting the next tick joins the load already running.
let loading: Promise<void> | null = null;
let loadingQuick = false;
let queuedFull: Promise<void> | null = null;
export let lastFull = 0;
export const loadAll = (quick = false): Promise<void> => {
  if (loading) {
    // A full load asked for while a quick one runs waits its turn instead of being dropped.
    if (quick || !loadingQuick) return loading;
    return (queuedFull ??= loading.then(() => {
      queuedFull = null;
      return loadAll(false);
    }));
  }
  loadingQuick = quick;
  if (!quick) lastFull = Date.now();
  return (loading = useWorkspace
    .getState()
    .refreshAll({ quick })
    .finally(() => {
      loading = null;
    }));
};
let loadingEngines: Promise<void> | null = null;
export const loadEngines = () =>
  (loadingEngines ??= useWorkspace.getState().refreshEngines().finally(() => {
    loadingEngines = null;
  }));
