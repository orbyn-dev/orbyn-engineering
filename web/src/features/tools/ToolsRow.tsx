import { Plug } from "lucide-react";
import type { Thread } from "../../api/types";
import { api } from "./api";
import { topics } from "../../lib/dataRefresh";
import { useAsync } from "../../lib/useAsync";
import { engineById, useWorkspace } from "../../product/state/workspace";
import { useUi } from "../../product/state/ui";
import { DRow } from "../../product/ui/DetailRows";
import { NOT_REPORTED_CODE } from "./notReported";
import { problemLine, summarize } from "./toolsModel";

/**
 * The Details popover's Tools row: how many are ready, what is not, and it opens the Tools tab. One honest state per engine:
 * an engine that does not list its tools says so, one that has not run yet says when it will fill in, and one that should
 * have answered and did not says it could not read. The tab says the same words.
 */
export function ToolsRow({ thread, onOpen }: { thread: Thread; onOpen?(): void }) {
  const engines = useWorkspace((state) => state.engines.data);
  const engineName = engineById(engines, thread.engine)?.name ?? thread.engine;
  const tools = useAsync(`thread-tools:${thread.id}`, (signal) => api.threadTools(thread.id, false, signal), { refreshOn: topics(`tools:${thread.id}`) });
  const reopen = useUi((state) => state.reopenPanelTab);
  const open = () => {
    reopen("tools");
    onOpen?.();
  };
  const data = tools.data;
  // A last good list that came back with an error is the error state, not a count.
  const summary = data?.reported && !data.error ? summarize(data) : null;
  const problems = summary ? problemLine(summary) : null;
  const failed = (summary?.failed ?? 0) > 0;
  const notReported = tools.status === "error" && tools.error.code === NOT_REPORTED_CODE;
  const unread = !summary && !notReported && (tools.status === "error" || Boolean(data?.error));
  return (
    <DRow
      icon={<Plug size={16} aria-hidden />}
      label="Tools"
      loading={tools.status === "loading"}
      onClick={open}
      ariaLabel="Tools"
      chevron
      value={
        summary ? (
          <span className="t2">{summary.starting > 0 ? `${summary.ready} ready · ${summary.starting} starting` : `${summary.ready} ready`}</span>
        ) : notReported ? (
          <span className="t3">Not reported by {engineName}</span>
        ) : unread ? (
          <span className="t3">Couldn’t read</span>
        ) : (
          <span className="t3">Shown once this engine has run</span>
        )
      }
      sub={problems ? <span className={failed ? "tg-card-bad" : "amb"}>{problems}</span> : summary ? engineName : undefined}
    />
  );
}
