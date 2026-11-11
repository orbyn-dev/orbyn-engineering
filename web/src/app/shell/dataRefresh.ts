import { startDataRefresh, registerRefresh, EVERYTHING } from "../../lib/dataRefresh";
import { reloadHeldPulls } from "../../features/pulls/model/pullStore";
import { useModelStore } from "../../product/state/modelStore";
import { useWorkspace } from "../../product/state/workspace";

/** The engine a `models:<engine>` topic names; `undefined` for `models` (every engine) or after a reconnect. */
const enginesOf = (heard: string[]): string[] | undefined => {
  if (heard.some((what) => what === "models" || what === EVERYTHING)) return undefined;
  return heard.map((what) => what.slice("models:".length));
};

/**
 * What the app itself keeps on screen, read again when the service says it changed: the workspace
 * and update status, the model lists, and the pull requests linked to threads. Screens register
 * their own reads (`useAsync`'s `refreshOn`). Quiet: nothing shows a loading state.
 */
export function startAppDataRefresh(): () => void {
  const stops = [
    startDataRefresh(),
    registerRefresh((what) => what === "workspace", () => useWorkspace.getState().refreshManaged({ quiet: true })),
    registerRefresh((what) => what === "updates", () => useWorkspace.getState().refreshUpdates({ quiet: true })),
    registerRefresh(
      (what) => what === "models" || what.startsWith("models:"),
      async (heard) => {
        const engines = enginesOf(heard);
        const { revalidate } = useModelStore.getState();
        if (engines === undefined) await revalidate();
        else await Promise.all(engines.map((engine) => revalidate(engine)));
      },
    ),
    registerRefresh(
      (what) => what === "pulls" || what.startsWith("pull:"),
      (heard) => heard.forEach((what) => reloadHeldPulls(what === EVERYTHING ? "pulls" : what)),
    ),
  ];
  return () => stops.forEach((stop) => stop());
}
