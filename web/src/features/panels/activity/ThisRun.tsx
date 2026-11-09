import { useMemo } from "react";
import { elapsed } from "../../../lib/format";
import { useNow } from "../../../lib/useNow";
import { useWorkspace } from "../../../product/state/workspace";
import { EngineGlyph } from "../../../product/ui/badges";
import { useThreadLogs } from "../../../product/state/threadLogs";
import { runningTools } from "../../../product/timeline/timeline";

/** The tool calls running now in this thread's run, newest first; nothing when none run. */
export function ThisRun({ threadId }: { threadId: string }) {
  const active = useWorkspace((state) => state.threads[threadId]?.active_run ?? null) !== null;
  const log = useThreadLogs((state) => state.logs[threadId]);
  const tools = useMemo(() => (active ? runningTools(log?.items ?? []) : []), [log, active]);
  const now = useNow(tools.length > 0);
  if (tools.length === 0) return null;
  return (
    <section className="this-run" aria-label="This run">
      <div className="details-head">
        <span>This run · {tools.length} running</span>
      </div>
      <ul className="detail-running">
        {[...tools].reverse().map((tool) => (
          <li key={tool.key}>
            <EngineGlyph name={tool.name} />
            <span>{tool.name}</span>
            <span className="detail-age">{elapsed(tool.ts, now)}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}
