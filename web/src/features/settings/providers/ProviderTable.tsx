import { Check, Plus, RefreshCw } from "lucide-react";
import { useRef, useState, type ReactNode } from "react";
import { listProviders, queryPreferences, updatePreferences } from "../api";
import type { ProviderEntry } from "../../../generated/ProviderEntry";
import type { ProviderList } from "../../../generated/ProviderList";
import { GROUPS, LABEL_TEXT, type PlaceKey } from "./providerLabels";
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

/** The status cell: what the person has to do, else that the provider is ready. A listed engine that was not found says so through its label. */
function Status({ engine, labelled }: { engine: EngineStatus; labelled: boolean }) {
  if (!engine.installed) return labelled ? null : <span className="set-pill warn">Not found</span>;
  if (engine.auth === "logged_out") {
    return <><span className="set-pill dot">Signed out</span>{engine.sign_in_command ? <span className="mono t3">{engine.sign_in_command}</span> : null}</>;
  }
  if (engine.latest && engine.version && engine.latest !== engine.version) return <span className="set-pill">Update {engine.latest}</span>;
  return <span className="set-pill"><Check size={13} aria-hidden />Ready</span>;
}

/** An instance is named `Provider · Name`: the provider leads the row, and the instance's own name opens the line under it. */
function rowNames(engine: EngineStatus): { title: string; label: string | null } {
  const at = engine.instance_of ? engine.name.indexOf(" · ") : -1;
  return at < 0 ? { title: engine.name, label: null } : { title: engine.name.slice(0, at), label: engine.name.slice(at + 3) };
}

/** The line under a provider's name: which instance, who is signed in, and what the service says about its state. */
const subtitle = (engine: EngineStatus, note?: string | null) => {
  const via = engine.tier === "acp" && !note ? "via ACP" : null;
  const who = engine.auth === "logged_out" ? "Not signed in" : engine.auth === "logged_in" ? engine.account ?? "Signed in" : null;
  return [rowNames(engine).label, via ?? (engine.tier === "acp" ? null : who), note].filter(Boolean).join(" · ") || null;
};

/** The labels the service gives a provider: what Orbyn has not seen or cannot control. */
function Labels({ entry }: { entry: ProviderEntry | undefined }) {
  return <>{entry?.labels.map((label) => <span key={label} className={label === "planned" ? "set-pill planned" : "set-pill dot"}>{LABEL_TEXT[label]}</span>)}</>;
}

/** The grouped list, read again whenever the engines change. `null` until the service answers or when it cannot. */
export function useProviderList(engines: EngineStatus[]): ProviderList | null {
  const key = engines.map((engine) => [engine.id, engine.installed, engine.auth, engine.version ?? "", engine.instance_of ?? "", engine.compatibility?.status ?? "",
    engine.capabilities.managed, engine.capabilities.approvals].join(":")).join();
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
    const note = subtitle(engine, entry?.note);
    const instance = store.instances.find((candidate) => candidate.id === engine.id);
    const openable = entry?.openable ?? true;
    return (
      <div key={engine.id} role="row" className={openable ? "set-tr set-tr-open" : "set-tr"} style={{ gridTemplateColumns: COLUMNS }} onClick={openable ? () => onOpen(engine.id) : undefined}>
        <span role="cell"><span className="set-mark" style={engine.accent ? { background: engine.accent, color: "#fff" } : undefined}>{engine.name.charAt(0).toUpperCase()}</span></span>
        <span role="cell">
          {openable
            ? <button type="button" className="set-row-open name" aria-label={engine.name} onClick={(event) => { event.stopPropagation(); onOpen(engine.id); }}>{rowNames(engine).title}</button>
            : <span className="name">{rowNames(engine).title}</span>}
          {note ? <><br /><span className="sub">{note}</span></> : null}
        </span>
        <span role="cell" className="t3">{engine.version ?? ""}</span>
        <span role="cell" className="set-states"><Status engine={engine} labelled={entry?.labels.includes("not_found") ?? false} /><Labels entry={entry} /></span>
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
  // Every registered engine has a row, whether or not the list has heard of it yet: a newly added or
  // misconfigured one must stay reachable. The list adds its labels and the rows that are not engines.
  const places = (() => {
    if (!catalog || catalog.providers.length === 0) return null;
    const rows: { place: PlaceKey; node: ReactNode }[] = [];
    for (const engine of engines) {
      const entry = catalog.providers.find((candidate) => candidate.id === engine.id && candidate.registered)
        ?? { id: engine.id, name: engine.name, group: engine.installed ? "in_use" : "not_found", labels: engine.installed ? [] : ["not_found"], registered: true, openable: true } satisfies ProviderEntry;
      const startable = entry.group === "in_use" || entry.group === "found" || entry.group === "more";
      const place: PlaceKey = startable ? (!row?.mixed && inUse.off.includes(engine.id) ? (entry.group === "more" ? "more" : "found") : "in_use") : entry.group;
      rows.push({ place, node: engineRow(engine, entry) });
    }
    for (const entry of catalog.providers.filter((candidate) => !candidate.registered)) rows.push({ place: entry.group === "in_use" ? "found" : entry.group, node: plainRow(entry) });
    return rows;
  })();
  return (
    <>
      {places ? GROUPS.map(({ place, heading, note }) => ({ heading, note, nodes: places.filter((one) => one.place === place).map((one) => one.node) })).filter((part) => part.nodes.length > 0).map(({ heading, note, nodes }) => (
        <div key={heading}>
          <p className="set-grp">{heading}</p>
          {note ? <p className="setting-line-desc">{note}</p> : null}
          <div className="set-tbl" role="table" aria-label={heading}>
            {head}
            {nodes}
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
