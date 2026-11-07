import {
  Activity,
  Copy,
  FileDiff,
  Gauge,
  Globe,
  ChevronDown,
  ChevronRight,
  MessagesSquare,
  Monitor,
  SquareTerminal,
  EllipsisVertical,
  Trash2,
  X,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState, type RefObject } from "react";
import { api, request } from "../../api/client";
import type { EngineLimits, EngineStatus, LocalServer, ProjectFileView, Thread } from "../../api/types";
import { compactAge, elapsed } from "../../lib/format";
import { showSideChat, sideChatsOf } from "../panels/sideChat";
import { toFailure, useAsync } from "../../lib/useAsync";
import type { ProjectScript } from "../../generated/ProjectScript";
import { runProjectAction } from "./runProjectAction";
import { ProjectActionIcon } from "../../ui/ProjectActionIcon";
import { usageShare } from "./ComposerExtras";
import { pluralize } from "./toolCalls";
import { DRow } from "./DetailRows";
import { useWaited as useWaitedFor } from "../../lib/motion";
import { useNow } from "../../lib/useNow";
import { navigate } from "../../state/route";
import { useThreadLogs } from "./threadLogs";
import { useUi } from "../../state/ui";
import { useWorkspace } from "../../state/workspace";
import { EngineGlyph } from "../../ui/badges";
import { Menu, type MenuItem } from "../../ui/Menu";
import { closingProps, Presence } from "../../ui/Presence";
import { usePopover } from "../../ui/usePopover";
import { Button, IconButton } from "../../ui/controls";
import { ConfirmDialog } from "../../ui/Dialog";
import { BranchRow } from "./GitControl";
import { OpenInEditor } from "./OpenInEditor";
import { PullsRow } from "./PullChip";
import { ThreadAutomations } from "./ThreadAutomations";
import { runningTools } from "./timeline";

function ThreadActions({ deletable, closing, panelRef, onDelete }: { deletable: boolean; closing?: boolean; panelRef: RefObject<HTMLDivElement | null>; onDelete(): void }) {
  return (
    <div className="menu" ref={panelRef} {...closingProps(closing)}>
      <Button tone="quiet" small icon={<Trash2 size={16} aria-hidden />} disabled={!deletable} onClick={onDelete}>
        Delete thread
      </Button>
    </div>
  );
}

function ThreadMenu({ thread }: { thread: Thread }) {
  const [open, setOpen] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const dropThread = useWorkspace((state) => state.dropThread);
  const menu = useRef<HTMLDivElement>(null);

  const panel = useRef<HTMLDivElement>(null);
  const popover = usePopover({ kind: "readonly", root: panel, active: open, trigger: () => menu.current?.querySelector("button"), onClose: () => setOpen(false) });

  return (
    <div className="menu-anchor" ref={menu}>
      <IconButton label="Thread actions" aria-expanded={open} onClick={() => setOpen(!open)}>
        <EllipsisVertical size={14} aria-hidden />
      </IconButton>
      <Presence>
        {open ? (
          <ThreadActions
            deletable={thread.active_run === null}
            panelRef={panel}
            onDelete={() => {
              popover.dismiss("select");
              setConfirming(true);
            }}
          />
        ) : null}
      </Presence>
      {confirming ? (
        <ConfirmDialog
          title="Delete thread?"
          confirmLabel="Delete thread"
          tone="danger"
          onClose={() => setConfirming(false)}
          onConfirm={async () => {
            await api.deleteThread(thread.id);
            dropThread(thread);
            navigate({ name: "home" });
          }}
        >
          <p>
            “{thread.title}” and its log are removed from Orbyn. Files on disk and any worktree stay
            where they are.
          </p>
        </ConfirmDialog>
      ) : null}
    </div>
  );
}

export const DETAIL_ROWS: { id: string; label: string }[] = [
  { id: "branch", label: "Branch" },
  { id: "changes", label: "Changes" },
  { id: "pulls", label: "Pull requests" },
  { id: "automations", label: "Automations" },
  { id: "servers", label: "Local servers" },
  { id: "limits", label: "Limits" },
  { id: "usage", label: "Usage" },
  { id: "scripts", label: "Scripts" },
  { id: "editor", label: "Open in editor" },
];

/** The rows as a checklist, for the card's menu and for Settings. */
export function detailsMenuItems(): MenuItem[] {
  const { detailsHidden, setPrefs } = useUi.getState();
  return [
    ...DETAIL_ROWS.map((row) => ({
      label: row.label,
      selected: !detailsHidden.includes(row.id),
      keepOpen: true,
      onSelect: () =>
        setPrefs({
          detailsHidden: detailsHidden.includes(row.id)
            ? detailsHidden.filter((id) => id !== row.id)
            : [...detailsHidden, row.id],
        }),
    })),
    { label: "Reset", separatorBefore: true, onSelect: () => setPrefs({ detailsHidden: ["usage"] }) },
  ];
}

function ServersRow({ threadId }: { threadId: string }) {
  const servers = useAsync(`details-servers:${threadId}`, (signal) =>
    request<LocalServer[]>("GET", `/api/threads/${encodeURIComponent(threadId)}/servers`, undefined, signal),
  );
  const togglePanel = useUi((state) => state.togglePanel);
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
      sub={list.length > 1 ? list.map(address).join(" · ") : undefined}
      onClick={list.length > 0 ? () => (only ? showInBrowser(`http://${address(only)}`) : togglePanel("browser")) : undefined}
    />
  );
}

function UsageRow({ threadId }: { threadId: string }) {
  const usage = useThreadLogs((state) => state.logs[threadId]?.usage);
  const share = usageShare(usage);
  if (!share) return null;
  return <DRow icon={<Gauge size={16} aria-hidden />} label="Usage" value={<span className="t2">{share.used >= 1000 ? `${Math.round(share.used / 1000)}k` : share.used} tokens</span>} sub="This thread" />;
}

function ScriptsRow({ threadId }: { threadId: string }) {
  const base = `/api/threads/${encodeURIComponent(threadId)}`;
  const scripts = useAsync(`details-scripts:${threadId}`, (signal) =>
    request<ProjectFileView>("GET", `${base}/scripts`, undefined, signal),
  );
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
              <button type="button" className="dscript" onClick={() => run(script)}>
                <ProjectActionIcon icon={script.icon} />
                <span>{script.name}</span>
              </button>
            </li>
          ))}
        </ul>
      ) : null}
    </DRow>
  );
}

function SideChatRows({ threadId }: { threadId: string }) {
  const threads = useWorkspace((state) => state.threads);
  const chats = useMemo(() => sideChatsOf(threads, threadId), [threads, threadId]);
  if (chats.length === 0) return null;
  return (
    <div className="details-section">
      <div className="details-head">
        <span>Side chats</span>
      </div>
      {chats.map((chat) => (
        <button key={chat.id} type="button" className="detail-row" onClick={() => showSideChat(chat.id)}>
          <MessagesSquare size={16} aria-hidden />
          <span>{chat.title}</span>
          <span className="detail-meta">{compactAge(chat.updated_at)}</span>
        </button>
      ))}
    </div>
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

/** What's left of each limit window for this thread's engine, when it reports one. */
function LimitsRow({ threadId, engine }: { threadId: string; engine: string }) {
  const limits = useAsync(`details-limits:${engine}`, () => {
    const kept = lastLimits.get(engine);
    if (kept && Date.now() - kept.at < LIMITS_CHECK_MS) return kept.limits;
    const fresh = api.threadLimits(threadId);
    lastLimits.set(engine, { at: Date.now(), limits: fresh });
    fresh.catch(() => lastLimits.delete(engine));
    return fresh;
  });
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

function RunningList({ threadId, active }: { threadId: string; active: boolean }) {
  const log = useThreadLogs((state) => state.logs[threadId]);
  const tools = useMemo(() => (active ? runningTools(log?.items ?? []) : []), [log, active]);
  const now = useNow(tools.length > 0);
  if (tools.length === 0) return null;
  return (
    <ul className="detail-running">
      {[...tools].reverse().map((tool) => (
        <li key={tool.key}>
          <EngineGlyph name={tool.name} />
          <span>{tool.name}</span>
          <span className="detail-age">{elapsed(tool.ts, now)}</span>
        </li>
      ))}
    </ul>
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

export function DetailsCard({
  thread,
  engine,
  compact,
  onClose,
}: {
  thread: Thread;
  engine: EngineStatus | null;
  /** On one-surface layouts the card also carries the panel shortcuts. */
  compact: boolean;
  onClose(): void;
}) {
  const remote = useRemoteLabel();
  const hidden = useUi((state) => state.detailsHidden);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [collapsed, setCollapsed] = useCollapsedCard();
  const shows = (row: string) => !hidden.includes(row);
  const log = useThreadLogs((state) => state.logs[thread.id]);
  const runningCount = useMemo(
    () => (thread.active_run ? runningTools(log?.items ?? []).length : 0),
    [log, thread.active_run],
  );
  const togglePanel = useUi((state) => state.togglePanel);
  const projectBranch = useWorkspace(
    (state) => state.managed.data?.targets.find((t) => t.project_id === thread.project_id)?.branch ?? null,
  );
  const branch = thread.worktree?.branch ?? projectBranch;
  const pushToast = useUi((state) => state.pushToast);
  const copyPath = () => {
    navigator.clipboard?.writeText(thread.cwd).then(
      () => pushToast("Folder path copied"),
      () => pushToast("Couldn't copy the path"),
    );
  };

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
      <div className="dcard" data-collapsed={collapsed || undefined} aria-label="Thread details rows" role="group">
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
            {shows("servers") ? <ServersRow threadId={thread.id} /> : null}
            {shows("limits") ? <LimitsRow threadId={thread.id} engine={thread.engine} /> : null}
            {shows("usage") ? <UsageRow threadId={thread.id} /> : null}
            {shows("scripts") ? <ScriptsRow threadId={thread.id} /> : null}
            {shows("editor") ? <OpenInEditor threadId={thread.id} /> : null}
          </>
        )}
      </div>
      {compact ? (
        <div className="details-section">
          <button type="button" className="detail-row" onClick={() => togglePanel("activity")}>
            <Activity size={16} aria-hidden />
            <span>Activity</span>
          </button>
          <button type="button" className="detail-row" onClick={() => togglePanel("terminal")}>
            <SquareTerminal size={16} aria-hidden />
            <span>Terminal</span>
          </button>
        </div>
      ) : null}
      <div className="details-section">
        <div className="detail-row detail-row-static">
          <Monitor size={16} aria-hidden />
          <span className="detail-dim">{remote}</span>
          <IconButton label={`Copy folder path ${thread.cwd}`} onClick={copyPath}>
            <Copy size={14} aria-hidden />
          </IconButton>
          <ThreadMenu thread={thread} />
          {compact ? (
            <IconButton label="Close details" onClick={onClose}>
              <X size={14} aria-hidden />
            </IconButton>
          ) : null}
        </div>
        <div className="detail-row detail-row-static">
          <EngineGlyph name={engine?.name ?? thread.engine} />
          <span>{engine?.name ?? thread.engine}</span>
          {engine && !engine.capabilities.managed ? <span className="detail-dim">Unmanaged</span> : null}
        </div>
      </div>
      <SideChatRows threadId={thread.id} />
      {runningCount > 0 ? (
        <div className="details-section">
          <div className="details-head">
            <span>This run · {runningCount} running</span>
          </div>
          <RunningList threadId={thread.id} active={thread.active_run !== null} />
        </div>
      ) : null}
    </aside>
  );
}

function useRemoteLabel(): string {
  const info = useAsync("details-info", (signal) => api.info(signal));
  return info.data?.remote ? "Remote service" : "This computer";
}
