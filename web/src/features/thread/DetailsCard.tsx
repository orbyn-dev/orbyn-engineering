import { Activity, ChevronDown, ChevronRight, FileDiff, Gauge, Globe, Play, SquareTerminal } from "lucide-react";
import { useEffect, useState } from "react";
import { api, listThreadScripts, listThreadServers } from "./api";
import type { EngineLimits, LocalServer, Thread } from "../../api/types";
import type { ProjectScript } from "../../generated/ProjectScript";
import { useWaited as useWaitedFor } from "../../lib/motion";
import { topics } from "../../lib/dataRefresh";
import { toFailure, useAsync } from "../../lib/useAsync";
import { useUi } from "../../product/state/ui";
import { useWorkspace } from "../../product/state/workspace";
import { Menu } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";
import { IconButton } from "../../ui/controls";
import { usageShare } from "./composer/ComposerExtras";
import { DRow } from "../../product/ui/DetailRows";
import { BranchRow } from "./GitControl";
import { OpenInEditor } from "./OpenInEditor";
import { PullsRow } from "./PullChip";
import { runProjectAction } from "../../product/composer/runProjectAction";
import { ThreadAutomations } from "./ThreadAutomations";
import { ToolsRow } from "../tools";
import { useThreadLogs } from "../../product/state/threadLogs";
import { pluralize } from "./toolCalls";
import { detailsMenuItems } from "../../product/ui/details";
function ServersRow({ threadId }: { threadId: string }) {
  const servers = useAsync(`details-servers:${threadId}`, (signal) =>
    listThreadServers(threadId, signal),
  );
  const showInBrowser = useUi((state) => state.showInBrowser);
  const waited = useWaitedFor(servers.status === "loading");
  const list = Array.isArray(servers.data) ? servers.data : [];
  const address = (server: LocalServer) => `localhost:${server.port}`;
  const only = list.length === 1 ? list[0] : undefined;
  return (
    <DRow
      icon={<Globe size={16} aria-hidden />}
      label="Local servers"
      loading={servers.status === "loading"}
      waited={waited}
      skeleton={96}
      value={
        servers.status === "error" ? (
          <span className="t3">Unavailable</span>
        ) : list.length === 0 ? (
          <span className="t3">None running</span>
        ) : only ? (
          <span className="mono">{address(only)}</span>
        ) : (
          <span className="t2">{list.length} servers</span>
        )
      }
      subLines={
        list.length > 1
          ? list.map((server) => (
              <button key={server.port} type="button" className="dlink mono" onClick={() => showInBrowser(`http://${address(server)}`)}>
                {address(server)}
              </button>
            ))
          : undefined
      }
      onClick={only ? () => showInBrowser(`http://${address(only)}`) : undefined}
    />
  );
}

/** 38k, 1.5k or 1.5M: a token count as the card reads it. */
export function tokensShort(count: number): string {
  const trim = (value: number) => value.toFixed(1).replace(/\.0$/, "");
  if (count >= 1e6) return `${trim(count / 1e6)}M`;
  if (count >= 1e4) return `${Math.round(count / 1e3)}k`;
  if (count >= 1e3) return `${trim(count / 1e3)}k`;
  return String(count);
}

function UsageRow({ threadId }: { threadId: string }) {
  const usage = useThreadLogs((state) => state.logs[threadId]?.usage);
  const share = usageShare(usage);
  if (!share) return null;
  return <DRow icon={<Gauge size={16} aria-hidden />} label="Usage" value={<span className="t2">{tokensShort(share.used)} tokens</span>} sub="This thread" />;
}

function ScriptsRow({ threadId }: { threadId: string }) {
  const scripts = useAsync(`details-scripts:${threadId}`, (signal) => listThreadScripts(threadId, signal));
  const pushToast = useUi((state) => state.pushToast);
  const [expanded, setExpanded] = useState(false);
  const waited = useWaitedFor(scripts.status === "loading");
  const list = scripts.data?.scripts ?? [];
  const run = (script: ProjectScript) => {
    void runProjectAction(threadId, script).catch((failure: unknown) => pushToast(`Couldn't run ${script.name}. ${toFailure(failure).message}`));
  };
  return (
    <DRow
      icon={<SquareTerminal size={16} aria-hidden />}
      label="Scripts"
      loading={scripts.status === "loading"}
      waited={waited}
      expanded={list.length > 0 ? expanded : undefined}
      onClick={list.length > 0 ? () => setExpanded(!expanded) : undefined}
      value={list.length === 0 ? <span className="t3">None in this project</span> : <span className="t2">{pluralize(list.length, "script", "scripts")}</span>}
      sub={list.length > 0 ? list.map((script) => script.name).join(" · ") : undefined}
    >
      {expanded && list.length > 0 ? (
        <ul className="dauto" aria-label="Scripts">
          {list.map((script) => (
            <li key={script.name} className="dtask">
              <span className="dtext">
                <b title={script.name}>{script.name}</b>
                <small className="mono">{script.command}</small>
              </span>
              <span className="end">
                <IconButton label={`Run ${script.name}`} onClick={() => run(script)}>
                  <Play size={14} aria-hidden />
                </IconButton>
              </span>
            </li>
          ))}
        </ul>
      ) : null}
    </DRow>
  );
}

function ChangesRow({ threadId }: { threadId: string }) {
  const version = useThreadLogs((state) => state.diffVersion[threadId] ?? 0);
  const diff = useAsync(`details-diff:${threadId}`, (signal) => api.diff(threadId, signal));
  const { reload } = diff;
  const togglePanel = useUi((state) => state.togglePanel);
  const showing = useUi((state) => state.panelOpen === true && state.panelTab === "diff");
  const waited = useWaitedFor(diff.status === "loading");

  useEffect(() => {
    if (version > 0) reload();
  }, [version, reload]);

  const files = diff.data ?? [];
  const added = files.reduce((sum, file) => sum + file.additions, 0);
  const removed = files.reduce((sum, file) => sum + file.deletions, 0);
  return (
    <DRow
      icon={<FileDiff size={16} aria-hidden />}
      label="Changes"
      loading={diff.status === "loading"}
      waited={waited}
      value={
        diff.status === "error" ? (
          <span className="t3">Unavailable</span>
        ) : files.length === 0 ? (
          <span className="t3">No changes</span>
        ) : (
          <>
            <span>
              <span className="diff-add">+{added}</span> <span className="diff-del">−{removed}</span>
            </span>{" "}
            <span className="t3">· {pluralize(files.length, "file", "files")}</span>
          </>
        )
      }
      onClick={() => {
        if (!showing) togglePanel("diff");
      }}
    />
  );
}

/** A limit window turns amber at this much left or less, and stays amber at 0%. */
const LIMIT_WARN_AT = 20;

/** Automatic checks stay this far apart, however often the card appears. */
const LIMITS_CHECK_MS = 5 * 60 * 1000;
const lastLimits = new Map<string, { at: number; limits: Promise<EngineLimits> }>();

/** Engines that give no limit window of their own: the row says so instead of staying empty. */
const LIMITS_NOT_REPORTED: readonly string[] = ["claude", "opencode"];

/** The Limits row: a window only for an engine that reports one (Codex); "Not reported" for Claude and OpenCode. */
function LimitsRow({ threadId, engine }: { threadId: string; engine: string }) {
  if (LIMITS_NOT_REPORTED.includes(engine)) {
    return <DRow icon={<Activity size={16} aria-hidden />} label="Limits" value={<span className="t3">Not reported</span>} />;
  }
  return <ReportedLimitsRow threadId={threadId} engine={engine} />;
}

/** What's left of each limit window for this thread's engine, when it reports one. */
function ReportedLimitsRow({ threadId, engine }: { threadId: string; engine: string }) {
  const limits = useAsync(`details-limits:${engine}`, (_signal, how) => {
    const kept = lastLimits.get(engine);
    // A change the service announced reads again whatever this card kept.
    if (!how.background && kept && Date.now() - kept.at < LIMITS_CHECK_MS) return kept.limits;
    const fresh = api.threadLimits(threadId);
    lastLimits.set(engine, { at: Date.now(), limits: fresh });
    fresh.catch(() => lastLimits.delete(engine));
    return fresh;
  }, { refreshOn: topics("limits", `limits:${engine}`) });
  const windows = limits.data?.windows ?? [];
  if (windows.length === 0) return null;
  return <DRow icon={<Activity size={16} aria-hidden />} label="Limits" value={<LimitValue windows={windows} />} />;
}

/** "5h 62% left · Weekly 88% left"; a window at 20% left or less is amber, and stays amber at 0%. */
export function LimitValue({ windows }: { windows: readonly { label: string; used_percent: number }[] }) {
  return (
    <span className="t2">
      {windows.map((window, at) => {
        const left = Math.max(100 - Math.round(window.used_percent), 0);
        return (
          <span key={window.label}>
            {at > 0 ? <span className="t3"> · </span> : null}
            <span className={left <= LIMIT_WARN_AT ? "amb" : undefined}>{`${window.label} ${left}% left`}</span>
          </span>
        );
      })}
    </span>
  );
}

/** The card's header collapses it to one line; the choice lasts for this window. */
function useCollapsedCard(): [boolean, (value: boolean) => void] {
  const [collapsed, set] = useState(() => {
    try {
      return sessionStorage.getItem("orbyn.details.collapsed") === "1";
    } catch {
      return false;
    }
  });
  return [
    collapsed,
    (value) => {
      set(value);
      try {
        sessionStorage.setItem("orbyn.details.collapsed", value ? "1" : "0");
      } catch {
        // Not remembered; the card still collapses.
      }
    },
  ];
}

export function DetailsCard({ thread }: { thread: Thread }) {
  const hidden = useUi((state) => state.detailsHidden);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [collapsed, setCollapsed] = useCollapsedCard();
  const shows = (row: string) => !hidden.includes(row);
  const projectBranch = useWorkspace(
    (state) => state.managed.data?.targets.find((t) => t.project_id === thread.project_id)?.branch ?? null,
  );
  const branch = thread.worktree?.branch ?? projectBranch;

  return (
    <aside
      className="details"
      aria-label="Thread details"
      onContextMenu={(event) => {
        event.preventDefault();
        setMenu({ x: event.clientX, y: event.clientY });
      }}
    >
      <Presence>{menu ? <Menu {...menu} label="Thread details rows" items={detailsMenuItems()} onClose={() => setMenu(null)} /> : null}</Presence>
      <div className="dcard" data-collapsed={collapsed || undefined}>
        <button type="button" className="dh" aria-label={collapsed ? "Expand card" : "Collapse card"} aria-expanded={!collapsed} onClick={() => setCollapsed(!collapsed)}>
          Details
          {collapsed ? <ChevronRight size={14} aria-hidden /> : <ChevronDown size={14} aria-hidden />}
        </button>
        {collapsed ? null : (
          <>
            {branch && shows("branch") ? <BranchRow thread={thread} branch={branch} /> : null}
            {shows("changes") ? <ChangesRow threadId={thread.id} /> : null}
            {shows("pulls") ? <PullsRow thread={thread} /> : null}
            {shows("automations") ? <ThreadAutomations threadId={thread.id} /> : null}
            {shows("tools") ? <ToolsRow thread={thread} /> : null}
            {shows("servers") ? <ServersRow threadId={thread.id} /> : null}
            {shows("limits") ? <LimitsRow threadId={thread.id} engine={thread.engine} /> : null}
            {shows("usage") ? <UsageRow threadId={thread.id} /> : null}
            {shows("scripts") ? <ScriptsRow threadId={thread.id} /> : null}
            {shows("editor") ? <OpenInEditor threadId={thread.id} /> : null}
          </>
        )}
      </div>
    </aside>
  );
}
