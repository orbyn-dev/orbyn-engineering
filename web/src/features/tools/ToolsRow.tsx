import { ChevronRight, Plug } from "lucide-react";
import type { Thread } from "../../api/types";
import { api } from "../../api/client";
import { useAsync } from "../../lib/useAsync";
import { useWaited as useWaitedFor } from "../../lib/motion";
import { engineById, useWorkspace } from "../../state/workspace";
import { useUi } from "../../state/ui";
import { DRow } from "../thread/DetailRows";
import { problemLine, summarize } from "./toolsModel";

/** The details card's Tools row: how many are ready, what is not, and it opens the Tools tab. */
export function ToolsRow({ thread }: { thread: Thread }) {
  const engines = useWorkspace((state) => state.engines.data);
  const engineName = engineById(engines, thread.engine)?.name ?? thread.engine;
  const tools = useAsync(`thread-tools:${thread.id}`, (signal) => api.threadTools(thread.id, false, signal));
  const waited = useWaitedFor(tools.status === "loading");
  const reopen = useUi((state) => state.reopenPanelTab);
  const open = () => reopen("tools");
  const data = tools.data;
  const summary = data?.reported ? summarize(data) : null;
  const problems = summary ? problemLine(summary) : null;
  const failed = (summary?.failed ?? 0) > 0;
  return (
    <DRow
      icon={<Plug size={14} aria-hidden />}
      label="Tools"
      loading={tools.status === "loading"}
      waited={waited}
      skeleton={54}
      onClick={open}
      ariaLabel="Tools"
      value={
        summary ? <span className="t2">{summary.ready} ready</span> : <span className="t3">Not reported yet</span>
      }
      sub={
        problems ? <span className={failed ? "tg-card-bad" : undefined}>{problems}</span> : engineName
      }
      extra={<ChevronRight size={14} aria-hidden className="tg-card-chev" />}
    />
  );
}
