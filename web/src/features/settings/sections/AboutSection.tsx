import { ChevronLeft, Search } from "lucide-react";
import { settingsLabel } from "../settingsSearch";
import { useState } from "react";
import { api, readTriagePlaybook } from "../api";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { Button } from "../../../ui/controls";
import licenses from "../licenses.json";
import { Segmented } from "../../../ui/Segmented";
import { SettingGroup, SettingRow } from "../rows/SettingRow";
import { navigate } from "../../../product/state/route";
import { LogoTile } from "../../../product/ui/Logo";
import { DRAFT_KEY, SCRATCH } from "../../../product/state/draftStore";
import { heading, Toggle } from "./parts";

/** The two lists of open-source packages, one for the interface and one for the service. */
function LicenceLists({ onBack }: { onBack(): void }) {
  const [list, setList] = useState<"scripts" | "crates">("scripts");
  const [filter, setFilter] = useState("");
  const rows = licenses[list].filter((entry) => entry.name.toLowerCase().includes(filter.trim().toLowerCase()));
  return (
    <>
      <div className="set-back">
        <Button small tone="quiet" icon={<ChevronLeft size={13} aria-hidden />} onClick={onBack}>About</Button>
        <h2 className="set-back-title">Open-source licences</h2>
      </div>
      <div className="set-licence-bar">
        <Segmented<"scripts" | "crates"> label="Licence list" value={list} onChange={setList}
          options={[{ value: "scripts", label: `App · ${licenses.scripts.length}` }, { value: "crates", label: `Service · ${licenses.crates.length}` }]} />
        <label className="set-find">
          <Search size={13} aria-hidden />
          <input aria-label="Find a package" placeholder="Find a package" value={filter} onChange={(event) => setFilter(event.target.value)} />
        </label>
      </div>
      <div className="set-tbl" role="table" aria-label="Packages">
        <div className="set-tr h" role="row" style={{ gridTemplateColumns: "minmax(0, 1fr) 110px 110px" }}>
          <span role="columnheader">Package</span><span role="columnheader">Version</span><span role="columnheader">Licence</span>
        </div>
        {rows.map((entry) => (
          <div key={entry.name} role="row" className="set-tr" style={{ gridTemplateColumns: "minmax(0, 1fr) 110px 110px" }}>
            <span role="cell">{entry.name}</span><span role="cell" className="t3">{entry.version}</span><span role="cell" className="t3">{entry.license}</span>
          </div>
        ))}
      </div>
    </>
  );
}

/** Whether the service looks for an app update on its own. Nothing is applied without a click. */
function AutomaticUpdateChecks() {
  const settings = useAsync("server-settings", (signal) => api.settings(signal), { keep: true });
  const [error, setError] = useState<string | null>(null);
  return (
    <>
      <Toggle
        title={settingsLabel("about-check-for-updates-automatically")}
        on={settings.data?.background_update_checks !== false}
        set={(on) => {
          if (!settings.data) return;
          void api.updateSettings((current) => ({ ...current, background_update_checks: on })).then(settings.setData, (failure: unknown) => setError(toFailure(failure).message));
        }}
      />
      {error ? <p className="field-error" role="alert">{error}</p> : null}
    </>
  );
}

/** Opens a draft holding the service's triage playbook; the person picks an agent and sends it. */
function TriageRow() {
  const pushToast = useUi((state) => state.pushToast);
  const start = async () => {
    try {
      const playbook = await readTriagePlaybook();
      useUi.getState().setDraft(DRAFT_KEY, playbook.prompt);
      navigate({ name: "new", projectId: SCRATCH });
    } catch (failure) {
      pushToast(`Couldn't start triage: ${toFailure(failure).message}`);
    }
  };
  return (
    <SettingRow title={settingsLabel("about-triage")} description="Starts a thread with a support playbook for this version. Nothing is sent until you send it.">
      <Button small onClick={() => void start()}>
        Start triage
      </Button>
    </SettingRow>
  );
}

/** The agent CLIs Orbyn found and their versions, with a re-check. */
function AgentClis() {
  const engines = useWorkspace((state) => state.engines.data);
  const refresh = useWorkspace((state) => state.refreshEngines);
  const [checking, setChecking] = useState(false);
  const found = (engines ?? []).filter((engine) => !engine.instance_of && engine.installed && engine.version);
  if (found.length === 0) return null;
  return (
    <SettingRow title="Agent CLIs" description={found.map((engine) => `${engine.name} ${engine.version}`).join(" · ")}>
      <Button small busy={checking} onClick={() => { setChecking(true); void api.refreshEngines().then(refresh).finally(() => setChecking(false)); }}>Check</Button>
    </SettingRow>
  );
}

/** The app update, once downloaded: the card says so and offers a restart, as Maintenance does. Later hides it until the page opens again. */
function useDownloadedUpdate() {
  const app = useWorkspace((state) => state.updates.data?.app);
  const [later, setLater] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const done = !!app?.latest && app.downloaded !== undefined && app.total !== undefined && Number(app.total) > 0
    && Number(app.downloaded) >= Number(app.total) && !!app.can_apply;
  if (!done || later) return null;
  return {
    latest: app.latest,
    error,
    later: () => setLater(true),
    restart: () => { setError(null); void api.applyAppUpdate().catch((problem: unknown) => setError(toFailure(problem).message)); },
  };
}

export function AboutSection() {
  const info = useAsync("about-info", (signal) => api.info(signal), { keep: true });
  const [licences, setLicences] = useState(false);
  const update = useDownloadedUpdate();
  if (licences) return <><h1 className="sr-only" tabIndex={-1} data-page-title>About</h1><LicenceLists onBack={() => setLicences(false)} /></>;
  return (
    <>
      {heading("About")}
      <div className="set-card">
        <LogoTile size={40} />
        <div>
          <b>Orbyn Engineering {info.data?.version ?? ""}</b>
          {update ? <><br /><span className="t3 sub">Update {update.latest} downloaded</span></> : null}
        </div>
        {update ? (
          <span className="set-card-end">
            <Button small tone="quiet" onClick={update.later}>Later</Button>
            <Button small tone="primary" onClick={update.restart}>Restart to update</Button>
          </span>
        ) : null}
      </div>
      {update?.error ? <p className="field-error" role="alert">{update.error}</p> : null}
      <SettingGroup>
        <AutomaticUpdateChecks />
        <AgentClis />
        <TriageRow />
        <SettingRow title="Open-source licences">
          <Button small onClick={() => setLicences(true)}>View</Button>
        </SettingRow>
        <SettingRow title="orbyn.dev" description="Website, documentation and release notes">
          <a className="btn btn-secondary btn-small" href="https://orbyn.dev" target="_blank" rel="noopener noreferrer">Open</a>
        </SettingRow>
      </SettingGroup>
    </>
  );
}
