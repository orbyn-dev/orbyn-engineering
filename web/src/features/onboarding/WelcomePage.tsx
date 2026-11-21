import { Check, Download, Folder, GitPullRequest, RefreshCw, TriangleAlert } from "lucide-react";
import { useState } from "react";
import type { EngineStatus } from "../../api/types";
import { relativeTime } from "../../lib/format";
import { toFailure } from "../../lib/useAsync";
import { navigate, routeHref } from "../../product/state/route";
import { useUi } from "../../product/state/ui";
import { useVisibleProjects, useWorkspace } from "../../product/state/workspace";
import { ProjectBadge } from "../../product/ui/badges";
import { Button } from "../../ui/controls";
import { Menu, type MenuItem } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";
import { TRACKER_NAMES, useTrackers, type TrackerRow } from "../issues";
import { useDefaultEngine } from "./useDefaultEngine";
import { agentRow, listedAgents, noEngineReady, recentProjects, type AgentRow } from "./welcomeModel";
import "./onboarding.css";

const MARK: Record<string, string> = { github: "GH", gitlab: "GL", azure_devops: "AZ", forgejo: "FJ", linear: "LN" };

/** Trackers: a token lives in the keychain behind the service and the page never sees one, so Connect goes to Settings. */
function TrackerAction({ rows }: { rows: readonly TrackerRow[] }) {
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const items: MenuItem[] = rows.map((row) => ({
    label: TRACKER_NAMES[row.tracker],
    content: (
      <span className="welcome-tracker">
        <span className="welcome-mark" aria-hidden>{MARK[row.tracker]}</span>
        <span>{TRACKER_NAMES[row.tracker]}{row.connected && row.scope ? <small> {"·"} {row.scope}</small> : null}</span>
        <small className="welcome-tracker-act">{row.connected ? "Connected" : "Connect"}</small>
      </span>
    ),
    onSelect: () => navigate({ name: "settings", section: "source-control" }),
  }));
  return (
    <>
      <button
        type="button"
        className="welcome-card"
        aria-haspopup="menu"
        aria-expanded={menu !== null}
        onClick={(event) => {
          const rect = event.currentTarget.getBoundingClientRect();
          setMenu(menu ? null : { x: rect.left, y: rect.bottom + 4 });
        }}
      >
        <GitPullRequest size={18} aria-hidden />
        <b>Connect a tracker</b>
        <span>GitHub, GitLab, Azure DevOps, Forgejo, Linear</span>
      </button>
      <Presence>{menu ? <Menu x={menu.x} y={menu.y} label="Connect a tracker" items={items} onClose={() => setMenu(null)} /> : null}</Presence>
    </>
  );
}

function AgentRowView({ row, chosen, canChoose, onUse }: { row: AgentRow; chosen: boolean; canChoose: boolean; onUse(engine: EngineStatus): Promise<void> }) {
  const [busy, setBusy] = useState(false);
  const pushToast = useUi((state) => state.pushToast);
  const { engine } = row;
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(engine.sign_in_command ?? "");
      pushToast(`Copied the ${engine.name} sign-in command. Run it in a terminal.`);
    } catch {
      pushToast("Couldn't copy the command.");
    }
  };
  return (
    <li className="welcome-agent-row">
      <span className="welcome-avatar" aria-hidden>{engine.name.slice(0, 1).toUpperCase()}</span>
      <span className="welcome-agent-name">
        <b>{engine.name}</b>
        <small>{row.detail}</small>
      </span>
      <span className="welcome-agent-state" data-state={row.state}>{row.status}</span>
      <span className="welcome-agent-act">
        {row.state === "ready" ? (
          chosen ? (
            <span className="welcome-ready"><Check size={14} aria-hidden />Used for new threads</span>
          ) : (
            <Button small disabled={!canChoose || busy} busy={busy} onClick={() => { setBusy(true); void onUse(engine).finally(() => setBusy(false)); }}>
              Use for new threads
            </Button>
          )
        ) : row.state === "signin" ? (
          engine.sign_in_command ? <Button small onClick={() => void copy()}>Sign in</Button> : <a className="btn btn-small" href={routeHref({ name: "settings", section: "providers" })}>Sign in</a>
        ) : (
          <a className="btn btn-small" href={routeHref({ name: "settings", section: "providers" })}>How to install</a>
        )}
      </span>
    </li>
  );
}

/**
 * The Welcome page: what a person sees first. Open or clone a project, connect a tracker, pick the engine new threads
 * start with, and go back to a recent project. Names, paths and engine text are drawn as text. Each button is one click, one action.
 */
export function WelcomePage() {
  const setPalette = useUi((state) => state.setPalette);
  const pushToast = useUi((state) => state.pushToast);
  const engines = useWorkspace((state) => state.engines.data);
  const refresh = useWorkspace((state) => state.refreshEngines);
  const projects = useVisibleProjects();
  const threads = useWorkspace((state) => state.threads);
  const trackers = useTrackers();
  const defaults = useDefaultEngine();
  const rows = listedAgents(engines ?? []).map(agentRow);
  const recent = recentProjects(projects ?? [], threads);
  const use = async (engine: EngineStatus) => {
    try {
      await defaults.choose(engine.id);
      pushToast(`New threads start on ${engine.name}.`);
    } catch (failure) {
      pushToast(`Couldn't change the engine. ${toFailure(failure).message}`);
    }
  };
  const open = (id: string, trusted: boolean) => navigate(trusted ? { name: "new", projectId: id } : { name: "trust", projectId: id, threadId: null });
  return (
    <div className="welcome-page" data-trackers={trackers.rows !== null || undefined}>
      <header>
        <h1 tabIndex={-1} data-page-title>Welcome to Orbyn Engineering</h1>
        <p>Open a project{trackers.rows !== null ? ", connect a tracker" : ""} and pick the engine your threads start with.</p>
      </header>
      {noEngineReady(engines) ? (
        <div className="welcome-banner" role="status">
          <TriangleAlert size={15} aria-hidden />
          <span>No engine is ready. Sign in to one below, or install Claude Code, Codex or OpenCode.</span>
          <Button small icon={<RefreshCw size={13} aria-hidden />} onClick={() => void refresh()}>Check again</Button>
        </div>
      ) : null}
      <div className="welcome-cards">
        <button type="button" className="welcome-card" onClick={() => setPalette("add-project")}>
          <Folder size={18} aria-hidden />
          <b>Open a project</b>
          <span>A folder on this computer</span>
        </button>
        <button type="button" className="welcome-card" onClick={() => setPalette("add-project-url")}>
          <Download size={18} aria-hidden />
          <b>Clone a repository</b>
          <span>Git URL, or owner/name on GitHub</span>
        </button>
        {trackers.rows !== null ? <TrackerAction rows={trackers.rows} /> : null}
      </div>
      <section aria-labelledby="welcome-agents">
        <h2 id="welcome-agents">Engine for new threads</h2>
        {engines === null ? (
          <p className="welcome-quiet" role="status">Looking for agents…</p>
        ) : (
          <ul className="welcome-agent-list" aria-label="Agents on this computer">
            {rows.map((row) => (
              <AgentRowView key={row.engine.id} row={row} chosen={defaults.engine === row.engine.id} canChoose={defaults.loaded} onUse={use} />
            ))}
          </ul>
        )}
      </section>
      <section aria-labelledby="welcome-projects">
        <h2 id="welcome-projects">Recent projects</h2>
        {recent.length === 0 ? (
          <p className="welcome-quiet">Projects you open or clone show up here.</p>
        ) : (
          <ul className="welcome-project-list">
            {recent.map(({ project, lastAt, threads: count }) => (
              <li key={project.id}>
                <div className="welcome-project-row" role="link" tabIndex={0} onClick={() => open(project.id, project.trusted)} onKeyDown={(event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); open(project.id, project.trusted); } }}>
                  <ProjectBadge name={project.name} project={project} />
                  <span className="welcome-project-name">
                    <b>{project.name}</b>
                    <span className="mono" title={project.path}>{project.path}</span>
                  </span>
                  <span className="welcome-quiet">{relativeTime(lastAt)}</span>
                  <span className="welcome-quiet">{count} {count === 1 ? "thread" : "threads"}</span>
                </div>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
