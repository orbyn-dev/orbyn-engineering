import { BookOpen, Plug, RefreshCw, Settings } from "lucide-react";
import { useState } from "react";
import { api } from "./api";
import { topics } from "../../lib/dataRefresh";
import { useAsync } from "../../lib/useAsync";
import { useWaited } from "../../lib/motion";
import { navigate } from "../../product/state/route";
import { engineById, useWorkspace } from "../../product/state/workspace";
import { Button, IconButton } from "../../ui/controls";
import { Empty } from "../../product/ui/states";
import { ToolsList } from "./ToolsList";
import { NOT_REPORTED_CODE } from "./notReported";
import { GROUP_SHORT, groupOf, summarize } from "./toolsModel";
import type { EngineTools } from "../../api/types";

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

/** Orbyn's own skills library, with the way into it. */
function SkillsLine({ count }: { count: number }) {
  return (
    <div className="tg-skills-line">
      <BookOpen size={14} aria-hidden />
      <span>Skills</span>
      <span className="t3">{count}</span>
      <Button small tone="quiet" onClick={() => navigate({ name: "skills" })}>
        Open Skills
      </Button>
    </div>
  );
}

/** What the last good read said, dimmed, so nothing the person knew disappears. Read-only. */
function LastRead({ data }: { data: EngineTools }) {
  const at = clock(data.checked_at);
  const rows = data.groups.filter((group) => GROUP_SHORT[group.id]);
  return (
    <div className="tg-last" aria-label="Last read">
      <div className="t3">{at ? `Last read ${at}` : "Last read"}</div>
      {rows.map((group) => {
        const ready = summarize({ ...data, groups: [group] }).ready;
        return (
          <div key={group.id} className="tg-last-row">
            <span>{GROUP_SHORT[group.id]}</span>
            <span className="t3">{ready} ready</span>
          </div>
        );
      })}
    </div>
  );
}

/** The side panel's Tools tab: what the thread's engine has, grouped, read-only. */
export function ToolsTab({ threadId, engine }: { threadId: string; engine: string }) {
  const engines = useWorkspace((state) => state.engines.data);
  const engineName = engineById(engines, engine)?.name ?? engine;
  const tools = useAsync(`thread-tools:${threadId}`, (signal) => api.threadTools(threadId, false, signal), { refreshOn: topics(`tools:${threadId}`) });
  const waited = useWaited(tools.status === "loading");
  const [checking, setChecking] = useState(false);
  const skillsCount = useAsync("tools-skills-count", (signal) => api.skills(signal));
  // Check again asks the engine itself; a plain reload could be answered from what is kept.
  const checkAgain = async () => {
    setChecking(true);
    try {
      tools.setData(await api.threadTools(threadId, true));
    } catch {
      await tools.reload();
    } finally {
      setChecking(false);
    }
  };

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
  // An engine with no list to give is not a failure: one plain state, and Orbyn's own library is still there.
  if (tools.status === "error" && tools.error.code === NOT_REPORTED_CODE) {
    return (
      <div className="tg-tools">
        <div className="tg-empty">
          <Plug size={22} aria-hidden className="tg-empty-icon" />
          <h4>Not reported by {engineName}</h4>
          <span>{engineName} doesn’t list its tools to Orbyn, so there is nothing to show here. Orbyn’s own skills are still available.</span>
          {skillsCount.data ? <SkillsLine count={skillsCount.data.skills.length} /> : null}
        </div>
      </div>
    );
  }
  const data = tools.data;
  // One error with its reason; what the person knew stays below, dimmed, with its time.
  if (tools.status === "error" || data?.error) {
    const reason = tools.status === "error" ? tools.error.message : (data?.error ?? "");
    return (
      <div className="tg-tools">
        <div className="tg-fail-box" role="alert">
          <span>
            <b>Couldn’t read the tools.</b>
            <br />
            {reason}
          </span>
          <Button small tone="quiet" onClick={() => void tools.reload()}>
            Try again
          </Button>
        </div>
        {data && data.groups.length > 0 ? <LastRead data={data} /> : null}
      </div>
    );
  }
  if (!data) return null;
  if (!data.reported) {
    const skills = groupOf(data, "skills");
    return (
      <div className="tg-tools">
        <Empty title="No tools reported for this run">
          <p className="t3">{engineName} reports its tools when a run starts. They appear here after the next message.</p>
        </Empty>
        {skills ? <ToolsList tools={{ ...data, groups: [skills], not_reported: [] }} engineName={engineName} mode="thread" /> : null}
      </div>
    );
  }
  return (
    <div className="tg-tools">
      <Header engineName={engineName} source={data.source} checkedAt={data.checked_at} busy={checking || tools.refreshing} onRefresh={data.source === "run" ? undefined : () => void checkAgain()} />
      <ToolsList tools={data} engineName={engineName} mode="thread" />
      <button type="button" className="tg-footlink" onClick={() => navigate({ name: "settings", section: "tools" })}>
        Tools and connectors settings
      </button>
    </div>
  );
}
