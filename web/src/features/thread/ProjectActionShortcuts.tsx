import { useEffect, useRef } from "react";
import { listThreadScripts } from "./api";
import { useCommand } from "../../product/shell/shortcuts";
import { toFailure, useAsync } from "../../lib/useAsync";
import { parseRoute } from "../../product/state/route";
import { useUi } from "../../product/state/ui";
import { actionCommand, actionIdFromCommand } from "../../product/composer/actionCommand";
import { runProjectAction } from "../../product/composer/runProjectAction";

function useActionRunner(threadId: string) {
  const pending = useRef(new Set<string>());
  const epoch = useRef(0);
  useEffect(() => {
    const cancel = () => { epoch.current += 1; };
    window.addEventListener("hashchange", cancel);
    return () => { cancel(); window.removeEventListener("hashchange", cancel); };
  }, [threadId]);
  const activeThread = () => {
    const route = parseRoute(window.location.hash);
    return route.name === "thread" && route.id === threadId && !document.querySelector("dialog[open]");
  };
  const run = (id: string) => {
    const observed = epoch.current;
    const requestKey = `${observed}:${id}`;
    const active = () => {
      return epoch.current === observed && activeThread();
    };
    if (!active()) return false;
    if (pending.current.has(requestKey)) return true;
    pending.current.add(requestKey);
    // A press picks the script to run, so it reads the catalog itself instead of joining a read that may be stale.
    void listThreadScripts(threadId, undefined, { fresh: true }).then(async (view) => {
      const script = view.scripts.find((entry) => entry.id === id);
      if (script && active()) await runProjectAction(threadId, script);
    }).catch((failure: unknown) => useUi.getState().pushToast(`Couldn't run that action. ${toFailure(failure).message}`))
      .finally(() => { pending.current.delete(requestKey); });
    return true;
  };
  return { run, activeThread };
}

function ActionShortcut({ id, run, activeThread }: { id: string; run(id: string): boolean; activeThread(): boolean }) {
  useCommand(actionCommand(id), () => run(id), activeThread);
  return null;
}

/** Register only actions available in this thread; unrelated project bindings stay inactive. */
export function ProjectActionShortcuts({ threadId }: { threadId: string }) {
  const scripts = useAsync(`action-shortcuts:${threadId}`, (signal) =>
    listThreadScripts(threadId, signal));
  const reload = scripts.reload;
  const { run, activeThread } = useActionRunner(threadId);
  useCommand("projectActions.pending", (command) => {
    if (scripts.status === "ready" && !scripts.refreshing) return false;
    const id = command && actionIdFromCommand(command);
    return id ? run(id) : false;
  }, (command) => activeThread() && (scripts.status !== "ready" || scripts.refreshing)
    && Boolean(command && actionIdFromCommand(command)));
  useCommand("projectActions.reload", () => void reload());
  useEffect(() => {
    window.addEventListener("focus", reload);
    return () => window.removeEventListener("focus", reload);
  }, [reload]);
  return scripts.data?.scripts.map((script) => script.id ? <ActionShortcut key={script.id} id={script.id} run={run} activeThread={activeThread} /> : null);
}
