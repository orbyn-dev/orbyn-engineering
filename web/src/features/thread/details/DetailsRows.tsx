import { Activity, FileDiff, Gauge, Globe, Play, SquareTerminal } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api, listThreadScripts, listThreadServers, stopThreadServer } from "../api";
import type { EngineLimits, FileDiff as DiffFile, LocalServer } from "../../../api/types";
import type { ProjectFileView } from "../../../generated/ProjectFileView";
import type { ProjectScript } from "../../../generated/ProjectScript";
import { useWaited as useWaitedFor } from "../../../lib/motion";
import { topics } from "../../../lib/dataRefresh";
import { toFailure, useAsync, type AsyncState } from "../../../lib/useAsync";
import { useUi } from "../../../product/state/ui";
import { useThreadLogs } from "../../../product/state/threadLogs";
import { DRow } from "../../../product/ui/DetailRows";
import { copyText } from "../../../product/ui/copyText";
import { runProjectAction } from "../../../product/composer/runProjectAction";
import { Menu, type MenuItem } from "../../../ui/Menu";
import { Presence } from "../../../ui/Presence";
import { IconButton } from "../../../ui/controls";
import { useDismissDetails as useDismiss } from "./DetailsDismiss";
import { usageShare } from "../composer/ComposerExtras";
import { pluralize } from "../toolCalls";


/** A row's read: its state and the way to read it again. */
type Read<T> = AsyncState<T> & { reload(): Promise<void> };

/** What the rows that can fail to read are built from. One failure reads as one row, so the loads live together. */
export interface DetailsLoads {
  diff: Read<DiffFile[]>;
  servers: Read<LocalServer[]>;
  scripts: Read<ProjectFileView | null>;
  /** Which of the three the card shows, by row id. */
  failed: string[];
  retry(): void;
}

/**
 * Reads the Changes, Local servers and Scripts rows. A row turned off in Settings reads nothing.
 * Changes reads again whenever the thread's diff does.
 */
export function useDetailsLoads(threadId: string, shows: (row: string) => boolean): DetailsLoads {
  const version = useThreadLogs((state) => state.diffVersion[threadId] ?? 0);
  const changes = shows("changes");
  const servers = shows("servers");
  const scripts = shows("scripts");
  const diff = useAsync<DiffFile[]>(`details-diff:${threadId}:${changes}`, (signal) => (changes ? api.diff(threadId, signal) : Promise.resolve([])));
  const running = useAsync<LocalServer[]>(`details-servers:${threadId}:${servers}`, (signal) => (servers ? listThreadServers(threadId, signal) : Promise.resolve([])));
  const project = useAsync<ProjectFileView | null>(`details-scripts:${threadId}:${scripts}`, (signal) => (scripts ? listThreadScripts(threadId, signal) : Promise.resolve(null)));
  const { reload } = diff;
  useEffect(() => {
    if (version > 0 && changes) reload();
  }, [version, changes, reload]);
  const failed = [
    ...(changes && diff.status === "error" ? ["changes"] : []),
    ...(servers && running.status === "error" ? ["servers"] : []),
    ...(scripts && project.status === "error" ? ["scripts"] : []),
  ];
  const retry = () => {
    if (failed.includes("changes")) void diff.reload();
    if (failed.includes("servers")) void running.reload();
    if (failed.includes("scripts")) void project.reload();
  };
  return { diff, servers: running, scripts: project, failed, retry };
}

export function ChangesRow({ diff }: { diff: Read<DiffFile[]> }) {
  const togglePanel = useUi((state) => state.togglePanel);
  const showing = useUi((state) => state.panelOpen === true && state.panelTab === "diff");
  const dismiss = useDismiss();
  const files = diff.data ?? [];
  const added = files.reduce((sum, file) => sum + file.additions, 0);
  const removed = files.reduce((sum, file) => sum + file.deletions, 0);
  return (
    <DRow
      icon={<FileDiff size={16} aria-hidden />}
      label="Changes"
      loading={diff.status === "loading"}
      value={
        files.length === 0 ? (
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
      chevron={files.length > 0}
      onClick={() => {
        if (!showing) togglePanel("diff");
        dismiss();
      }}
    />
  );
}

const address = (server: LocalServer) => `localhost:${server.port}`;
/** More than this many servers read as a count with the first few addresses, and the list opens in a menu. */
const SERVERS_SHOWN = 3;

export function ServersRow({ threadId, servers }: { threadId: string; servers: Read<LocalServer[]> }) {
  const showInBrowser = useUi((state) => state.showInBrowser);
  const pushToast = useUi((state) => state.pushToast);
  const dismiss = useDismiss();
  const waited = useWaitedFor(servers.status === "loading");
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const host = useRef<HTMLDivElement>(null);
  const list = Array.isArray(servers.data) ? servers.data : [];
  const only = list.length === 1 ? list[0] : undefined;
  const many = list.length > SERVERS_SHOWN;
  const open = (server: LocalServer) => {
    showInBrowser(`http://${address(server)}`);
    dismiss();
  };
  const items: MenuItem[] = list.map((server) => ({
    label: address(server),
    children: [
      { label: "Open", onSelect: () => open(server) },
      { label: "Copy", onSelect: () => void copyText(address(server), "Copied address") },
      ...(server.in_checkout
        ? [
            {
              label: "Stop",
              destructive: true,
              onSelect: () => {
                stopThreadServer(threadId, server.pid).then(
                  () => servers.reload(),
                  (failure: unknown) => pushToast(`Couldn't stop it. ${toFailure(failure).message}`),
                );
              },
            },
          ]
        : []),
    ],
  }));
  const aloneOrFew = list.length > 1 && !many;
  return (
    <div ref={host}>
      <DRow
        icon={<Globe size={16} aria-hidden />}
        label="Local servers"
        loading={servers.status === "loading"}
        waited={waited}
        skeleton={96}
        value={
          list.length === 0 ? (
            <span className="t3">None running</span>
          ) : only ? (
            <span className="mono">{address(only)}</span>
          ) : many ? (
            <span className="t2">{list.length} running</span>
          ) : (
            <span className="t2">{list.length} servers</span>
          )
        }
        sub={
          many ? (
            <span className="mono">
              {[address(list[0]!), ...list.slice(1, SERVERS_SHOWN).map((server) => `:${server.port}`), `+${list.length - SERVERS_SHOWN} more`].join(" · ")}
            </span>
          ) : undefined
        }
        subLines={
          aloneOrFew
            ? list.map((server) => (
                <button key={server.port} type="button" className="dlink mono" onClick={() => open(server)}>
                  {address(server)}
                </button>
              ))
            : undefined
        }
        chevron={many}
        onClick={
          many
            ? () => {
                const rect = host.current?.getBoundingClientRect();
                setMenu({ x: (rect?.right ?? 0) - 12, y: (rect?.bottom ?? 0) + 2 });
              }
            : only
              ? () => open(only)
              : undefined
        }
      />
      <Presence>{menu ? <Menu {...menu} label="Local servers" items={items} onClose={() => setMenu(null)} /> : null}</Presence>
    </div>
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

export function UsageRow({ threadId }: { threadId: string }) {
  const usage = useThreadLogs((state) => state.logs[threadId]?.usage);
  const share = usageShare(usage);
  if (!share) return null;
  return <DRow icon={<Gauge size={16} aria-hidden />} label="Usage" value={<span className="t2">{tokensShort(share.used)} tokens</span>} sub="This thread" />;
}

export function ScriptsRow({ threadId, scripts }: { threadId: string; scripts: Read<ProjectFileView | null> }) {
  const pushToast = useUi((state) => state.pushToast);
  const dismiss = useDismiss();
  const [expanded, setExpanded] = useState(false);
  const list = scripts.data?.scripts ?? [];
  const run = (script: ProjectScript) => {
    void runProjectAction(threadId, script).then(dismiss, (failure: unknown) => pushToast(`Couldn't run ${script.name}. ${toFailure(failure).message}`));
  };
  return (
    <DRow
      icon={<SquareTerminal size={16} aria-hidden />}
      label="Scripts"
      loading={scripts.status === "loading"}
      expanded={list.length > 0 ? expanded : undefined}
      chevron={false}
      onClick={list.length > 0 ? () => setExpanded(!expanded) : undefined}
      value={list.length === 0 ? <span className="t3">None in this project</span> : <span className="t2">{pluralize(list.length, "script", "scripts")}</span>}
      sub={list.length > 0 && !expanded ? list.map((script) => script.name).join(" · ") : undefined}
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

/** Automatic checks stay this far apart, however often the card appears. */
const LIMITS_CHECK_MS = 5 * 60 * 1000;
const lastLimits = new Map<string, { at: number; limits: Promise<EngineLimits> }>();

/** The Limits row: a window only for Codex, which reports one. Claude, OpenCode and any custom engine read "Not reported". */
export function LimitsRow({ threadId, engine }: { threadId: string; engine: string }) {
  if (engine !== "codex") {
    return <DRow icon={<Activity size={16} aria-hidden />} label="Limits" value={<span className="t3">Not reported</span>} />;
  }
  return <ReportedLimitsRow threadId={threadId} engine={engine} />;
}

/** What's left of each limit window for this thread's engine, when it reports one. */
function ReportedLimitsRow({ threadId, engine }: { threadId: string; engine: string }) {
  const limits = useAsync(
    `details-limits:${engine}`,
    (_signal, how) => {
      const kept = lastLimits.get(engine);
      // A change the service announced reads again whatever this card kept.
      if (!how.background && kept && Date.now() - kept.at < LIMITS_CHECK_MS) return kept.limits;
      const fresh = api.threadLimits(threadId);
      lastLimits.set(engine, { at: Date.now(), limits: fresh });
      fresh.catch(() => lastLimits.delete(engine));
      return fresh;
    },
    { refreshOn: topics("limits", `limits:${engine}`) },
  );
  const windows = limits.data?.windows ?? [];
  if (windows.length === 0) return null;
  return <DRow icon={<Activity size={16} aria-hidden />} label="Limits" value={<LimitValue windows={windows} />} />;
}

/**
 * "5h 62% left · Weekly 88% left". Every window keeps the same ink, 0% left included: amber is for what needs the person
 * and nothing has failed, and the thread's limit banner carries the action.
 */
export function LimitValue({ windows }: { windows: readonly { label: string; used_percent: number }[] }) {
  return (
    <span className="row1">
      {windows.map((window, at) => {
        const left = Math.max(100 - Math.round(window.used_percent), 0);
        return (
          <span key={window.label} className="t2">
            {at > 0 ? <span className="t4">· </span> : null}
            {`${window.label} ${left}% left`}
          </span>
        );
      })}
    </span>
  );
}
