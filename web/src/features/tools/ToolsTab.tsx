import { RefreshCw, Settings } from "lucide-react";
import { api } from "../../api/client";
import { useAsync } from "../../lib/useAsync";
import { useWaited } from "../../lib/motion";
import { navigate } from "../../state/route";
import { engineById, useWorkspace } from "../../state/workspace";
import { Button, IconButton } from "../../ui/controls";
import { Empty } from "../../ui/states";
import { ToolsList } from "./ToolsList";
import { groupOf } from "./toolsModel";

const clock = (iso: string | null | undefined) => {
  const date = iso ? new Date(iso) : null;
  return date && !Number.isNaN(date.getTime()) ? date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false }) : null;
};

/** One engine's header line: where the list came from, when, and the two buttons. */
function Header({ engineName, source, checkedAt, onRefresh, busy }: { engineName: string; source: string; checkedAt: string | null | undefined; onRefresh?(): void; busy: boolean }) {
  const at = clock(checkedAt);
  return (
    <div className="tg-eng">
      <span className="tg-badge" aria-hidden>
        {engineName.slice(0, 1)}
      </span>
      <span>{source === "run" ? `${engineName} · reported when this run started` : `${engineName} · checked${at ? ` ${at}` : ""}`}</span>
      {onRefresh ? (
        <IconButton label="Check again" onClick={onRefresh} disabled={busy}>
          <RefreshCw size={13} className={busy ? "spin" : undefined} aria-hidden />
        </IconButton>
      ) : null}
      <IconButton label="Tools and connectors settings" onClick={() => navigate({ name: "settings", section: "tools" })}>
        <Settings size={13} aria-hidden />
      </IconButton>
    </div>
  );
}

/** The side panel's Tools tab: what the thread's engine has, grouped, read-only. */
export function ToolsTab({ threadId, engine }: { threadId: string; engine: string }) {
  const engines = useWorkspace((state) => state.engines.data);
  const engineName = engineById(engines, engine)?.name ?? engine;
  const tools = useAsync(`thread-tools:${threadId}`, (signal) => api.threadTools(threadId, false, signal));
  const waited = useWaited(tools.status === "loading");

  if (tools.status === "loading") {
    return (
      <div className="tg-tools" aria-busy="true">
        <span className="sr-only" role="status">Reading the tools</span>
        {waited
          ? ["Your MCP servers", "Connectors", "Plugins"].map((label) => (
              <div key={label} className="tg-group" aria-hidden>
                <div className="tg-head"><span className="tg-toggle">{label}</span></div>
                <div className="tg-row"><span className="tg-skel" style={{ width: 14, height: 14 }} /><span className="tg-name"><span className="tg-skel" style={{ width: 110 }} /><span className="tg-skel" style={{ width: 210 }} /></span></div>
              </div>
            ))
          : null}
      </div>
    );
  }
  if (tools.status === "error" || !tools.data) {
    return (
      <div className="tg-tools">
        <Empty title="Couldn’t read the tools">
          <p className="t3">Try again.</p>
          <Button small onClick={() => void tools.reload()}>Try again</Button>
        </Empty>
      </div>
    );
  }
  const data = tools.data;
  if (!data.reported) {
    const skills = groupOf(data, "skills");
    return (
      <div className="tg-tools">
        <Empty title="No tools reported yet">
          <p className="t3">{engineName} reports its tools when a run starts. Send a message and they appear here.</p>
        </Empty>
        {skills ? <ToolsList tools={{ ...data, groups: [skills], not_reported: [] }} engineName={engineName} mode="thread" /> : null}
      </div>
    );
  }
  return (
    <div className="tg-tools">
      <Header engineName={engineName} source={data.source} checkedAt={data.checked_at} busy={tools.refreshing} onRefresh={data.source === "run" ? undefined : () => void tools.reload()} />
      {data.error ? <p className="tg-note" role="status">{data.error} The list is from {clock(data.checked_at) ?? "earlier"}.</p> : null}
      <ToolsList tools={data} engineName={engineName} mode="thread" />
      <button type="button" className="tg-footlink" onClick={() => navigate({ name: "settings", section: "tools" })}>
        Tools and connectors settings
      </button>
    </div>
  );
}
