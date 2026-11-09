import { ChevronLeft, Search } from "lucide-react";
import { settingsLabel } from "../settingsSearch";
import { useState } from "react";
import { api, request } from "../../../api/client";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { useUi } from "../../../state/ui";
import { useWorkspace } from "../../../state/workspace";
import { Button } from "../../../ui/controls";
import licenses from "../licenses.json";
import { Segmented } from "../../../ui/Segmented";
import { SettingGroup, SettingRow } from "../../../ui/SettingRow";
import type { TriagePlaybook } from "../../../generated/TriagePlaybook";
import { navigate } from "../../../state/route";
import { LogoTile } from "../../../ui/Logo";
import { DRAFT_KEY, SCRATCH } from "../../../state/draftStore";
import { heading, Toggle } from "./parts";

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
        description="Same setting as Background checks in Maintenance"
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
      const playbook = await request<TriagePlaybook>("GET", "/api/triage/playbook");
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

/** The service reports Rust's OS id, such as `macos`. */
const PLATFORM_NAMES: Record<string, string> = { macos: "macOS", windows: "Windows", linux: "Linux" };
export const platformName = (id: string) => PLATFORM_NAMES[id] ?? id;

export function AboutSection() {
  const info = useAsync("about-info", (signal) => api.info(signal), { keep: true });
  const [licences, setLicences] = useState(false);
  if (licences) return <><h1 className="sr-only" tabIndex={-1} data-page-title>About</h1><LicenceLists onBack={() => setLicences(false)} /></>;
  return (
    <>
      {heading("About")}
      <div className="set-card">
        <LogoTile size={40} />
        <div>
          <b>Orbyn Engineering {info.data?.version ?? ""}</b>
          <br /><span className="t3 sub">Platform {info.data ? platformName(info.data.platform) : "Unknown"}</span>
        </div>
      </div>
      <SettingGroup>
        <AutomaticUpdateChecks />
        <AgentClis />
        <TriageRow />
        <SettingRow title="Open-source licences" description="The app and the service">
          <Button small onClick={() => setLicences(true)}>View lists</Button>
        </SettingRow>
      </SettingGroup>
    </>
  );
}
