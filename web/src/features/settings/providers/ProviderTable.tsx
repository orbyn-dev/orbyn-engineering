import { Check, Plus, RefreshCw } from "lucide-react";
import { useRef, useState } from "react";
import { listProviders, queryPreferences, updatePreferences } from "../api";
import type { ProviderEntry } from "../../../generated/ProviderEntry";
import type { ProviderList } from "../../../generated/ProviderList";
import { GROUPS, LABEL_TEXT } from "./providerLabels";
import type { EngineStatus } from "../../../api/types";
import type { PreferenceEdit } from "../../../generated/PreferenceEdit";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { useUi } from "../../../product/state/ui";
import { Button } from "../../../ui/controls";
import type { InstanceStore } from "./ProviderInstances";
import { PreferenceInheritance } from "../rows/PreferenceInheritance";
import { useSettingsScope } from "../rows/SettingsScopeContext";

const KEY = "disabled_engines";

/** Which providers new threads may use here: the list holds the ones turned off. */
function useProvidersInUse() {
  const scope = useSettingsScope();
  const query = scope?.active ? scope.query : {};
  const list = useAsync(JSON.stringify(["in-use", query]), (signal) =>
    queryPreferences({ scope: query, keys: [KEY] }, signal), { keep: true });
  const [error, setError] = useState<string | null>(null);
  const pending = useRef(false);
  const row = list.data?.rows.find((entry) => entry.key === KEY);
  const off = Array.isArray(row?.value) ? row.value.filter((id): id is string => typeof id === "string") : [];
  const blocked = !row || list.refreshing || scope?.loading === true || scope?.catalog?.selection.kind === "unavailable";
  const write = async (edit: PreferenceEdit) => {
    if (!row || !list.data || pending.current || blocked) return;
    pending.current = true;
    setError(null);
    try {
      await updatePreferences({ scope: query, membership_version: list.data.catalog.version,
        if_match: { [KEY]: row.revision }, edits: { [KEY]: edit } });
      useUi.getState().bumpPreferencesRevision();
      list.reload();
    } catch (failure) { setError(toFailure(failure).message); } finally { pending.current = false; }
  };
  const set = (id: string, on: boolean) => {
    const base = row?.mixed ? [] : off;
    return write({ op: "set", value: on ? base.filter((entry) => entry !== id) : [...new Set([...base, id])] });
  };
  return { row, off, blocked, error, set, reset: (ids: string[]) => write({ op: "reset_overrides", project_ids: ids }), reload: list.reload, scope };
}

/** The status cell: what the person has to do, else that the provider is ready. */
function Status({ engine }: { engine: EngineStatus }) {
  if (!engine.installed) return <span className="set-pill warn">Not installed</span>;
  if (engine.auth === "logged_out" && engine.sign_in_command) return <span className="mono t3">{engine.sign_in_command}</span>;
  if (engine.latest && engine.version && engine.latest !== engine.version) return <span className="set-pill">Update {engine.latest}</span>;
  return <span className="set-pill"><Check size={13} aria-hidden />Ready</span>;
}

/** An instance is named `Provider · Name`: the provider leads the row, and the instance's own name opens the line under it. */
function rowNames(engine: EngineStatus): { title: string; label: string | null } {
  const at = engine.instance_of ? engine.name.indexOf(" · ") : -1;
  return at < 0 ? { title: engine.name, label: null } : { title: engine.name.slice(0, at), label: engine.name.slice(at + 3) };
}

/** The line under a provider's name: which instance, and who is signed in. */
const subtitle = (engine: EngineStatus) => {
  const status = engine.tier === "acp" ? "via ACP" : engine.auth === "logged_out" ? "Not signed in" : engine.auth === "logged_in" ? engine.account ?? "Signed in" : null;
  return [rowNames(engine).label, status].filter(Boolean).join(" · ") || null;
};

/** The labels the service gives a provider: what Orbyn has not seen or cannot control. */
function Labels({ entry }: { entry: ProviderEntry | undefined }) {
  return <>{entry?.labels.map((label) => <span key={label} className={label === "planned" ? "set-pill planned" : "set-pill dot"}>{LABEL_TEXT[label]}</span>)}</>;
}

/** The grouped list, read again whenever the engines change. `null` until the service answers or when it cannot. */
export function useProviderList(engines: EngineStatus[]): ProviderList | null {
  const key = engines.map((engine) => `${engine.id}:${engine.installed}:${engine.auth}:${engine.version ?? ""}`).join();
  const list = useAsync(`provider-list:${key}`, (signal) => listProviders(signal), { keep: true });
  return list.data;
}

const COLUMNS = "28px minmax(0, 1fr) 90px minmax(130px, auto) 60px 72px";

/** One row per provider and provider instance. A row opens its detail; Edit and In use belong to instances. */
export function ProviderTable({ engines, store, onOpen, onEdit, catalog = null }: {
  engines: EngineStatus[];
  store: InstanceStore;
  onOpen(id: string): void;
  onEdit(id: string): void;
  /** The grouped list. Without it, one flat table of the engines. */
  catalog?: ProviderList | null;
}) {
  const inUse = useProvidersInUse();
  const row = inUse.row;
  const all = inUse.scope?.catalog?.selection.kind === "all_projects";
  const head = (
    <div role="row" className="set-tr h" style={{ gridTemplateColumns: COLUMNS }}>
      <span /><span role="columnheader">Provider</span><span role="columnheader">Version</span><span role="columnheader">Status</span><span />
      <span role="columnheader" className="set-inuse-head">In use{row?.project_scoped ? (
        <PreferenceInheritance row={{ key: KEY, title: "Providers in use" }} view={row.members[0]?.view ?? row.environment} global={row.environment}
          members={row.members} mixed={row.mixed} overrides={all ? row.overrides : []} disabled={inUse.blocked}
          onResetOverrides={(ids) => void inUse.reset(ids)}
          onOpenOverride={(id) => {
            const group = inUse.scope?.catalog?.groups.find((entry) => entry.members.some((member) => member.project_id === id));
            if (group) inUse.scope?.select({ kind: "checkout", group_key: group.key, project_id: id });
          }} />
      ) : null}</span>
    </div>
  );
  const engineRow = (engine: EngineStatus, entry?: ProviderEntry) => {
    const note = entry?.note ?? subtitle(engine);
    const instance = store.instances.find((candidate) => candidate.id === engine.id);
    return (
      <div key={engine.id} role="row" className="set-tr set-tr-open" style={{ gridTemplateColumns: COLUMNS }} onClick={() => onOpen(engine.id)}>
        <span role="cell"><span className="set-mark" style={engine.accent ? { background: engine.accent, color: "#fff" } : undefined}>{engine.name.charAt(0).toUpperCase()}</span></span>
        <span role="cell">
          <button type="button" className="set-row-open name" aria-label={engine.name} onClick={(event) => { event.stopPropagation(); onOpen(engine.id); }}>{rowNames(engine).title}</button>
          {note ? <><br /><span className="sub">{note}</span></> : null}
        </span>
        <span role="cell" className="t3">{engine.version ?? ""}</span>
        <span role="cell" className="set-states"><Status engine={engine} /><Labels entry={entry} /></span>
        <span role="cell">{instance ? <Button small aria-label={`Edit ${engine.name}`} onClick={(event) => { event.stopPropagation(); onEdit(engine.id); }}>Edit</Button> : null}</span>
        <span role="cell" onClick={(event) => event.stopPropagation()}>
          {row ? <input type="checkbox" role="switch" aria-label={`Use ${engine.name}`} aria-checked={row.mixed ? "mixed" : !inUse.off.includes(engine.id)}
            ref={(element) => { if (element) element.indeterminate = row.mixed; }} checked={!row.mixed && !inUse.off.includes(engine.id)} disabled={inUse.blocked}
            onChange={(event) => void inUse.set(engine.id, event.target.checked)} /> : null}
        </span>
      </div>
    );
  };
  // A row with no engine behind it has nothing to open, sign in to or switch: only its words.
  const plainRow = (entry: ProviderEntry) => (
    <div key={entry.id} role="row" className="set-tr" style={{ gridTemplateColumns: COLUMNS }}>
      <span role="cell"><span className="set-mark">{entry.name.charAt(0).toUpperCase()}</span></span>
      <span role="cell"><span className="name">{entry.name}</span>{entry.note ? <><br /><span className="sub">{entry.note}</span></> : null}</span>
      <span role="cell" /><span role="cell" className="set-states"><Labels entry={entry} /></span><span role="cell" /><span role="cell" />
    </div>
  );
  // A real list always ends with the engines not built, so an empty one means the service gave none.
  const grouped = catalog && catalog.providers.length > 0
    ? GROUPS.map(({ group, heading }) => ({ heading, entries: catalog.providers.filter((entry) => entry.group === group) })).filter((part) => part.entries.length > 0)
    : null;
  return (
    <>
      {grouped ? grouped.map(({ heading, entries }, at) => (
        <div key={heading}>
          <p className="set-grp">{heading}</p>
          <div className="set-tbl" role="table" aria-label={heading}>
            {at === 0 ? head : null}
            {entries.map((entry) => {
              const engine = entry.registered ? engines.find((candidate) => candidate.id === entry.id) : undefined;
              return engine ? engineRow(engine, entry) : entry.registered ? null : plainRow(entry);
            })}
          </div>
        </div>
      )) : (
        <div className="set-tbl" role="table" aria-label="Providers">
          {head}
          {engines.map((engine) => engineRow(engine))}
        </div>
      )}
      {inUse.error ? <p className="field-error" role="alert">{inUse.error} <Button small onClick={inUse.reload}>Reload</Button></p> : null}
    </>
  );
}

export function ProviderListActions({ onAdd, onRecheck, checking }: { onAdd(): void; onRecheck(): void; checking: boolean }) {
  return (
    <div className="set-tools">
      <Button small onClick={onAdd}><Plus size={13} aria-hidden />Add provider</Button>
      <Button small busy={checking} onClick={onRecheck}><RefreshCw size={13} aria-hidden />Check again</Button>
    </div>
  );
}
