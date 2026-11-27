import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { RouteTier } from "../../../../generated/RouteTier";
import { useAsync } from "../../../../lib/useAsync";
import { useWorkspace } from "../../../../product/state/workspace";
import { CrewRoleNote, type CrewRoleFields } from "../../../cursorthread";
import { useConnection } from "../../../../product/state/connection";
import { deferNavigation, navigationGuarded, registerNavigationGuard, type NavigationResume, type NavigationGuardOwner } from "../../../../product/state/navigationGuard";
import { goToHash } from "../../../../product/state/route";
import { Button } from "../../../../ui/controls";
import { ErrorState, Loading } from "../../../../product/ui/states";
import { reviewControlApi, controlScope, assertControlScope } from "../../shared/reviewControlApi";
import { copyTiers, ownTier, sameTiers, tierProblem } from "./engineTierDraft";
import { initialTierEditor, patchTierEditor, putTierEditor, reconcileTiers, saveTiers, tierBusy, useTierEditors } from "./engineTierWrites";
import "./review-controls.css";

const tierName = (tier: string) => tier === "worker" ? "Builder" : tier === "lead" ? "Lead" : "Scout";
/** The approved inline editor, not a permission or agent-launch surface. */
export function EngineTiers() {
  const scope = controlScope();
  const read = useAsync(`crew-engine-tiers:${scope}`, (signal) => reviewControlApi.tiers(signal));
  const editor = useTierEditors((s) => s.editors[scope]);
  const remote = useConnection((s) => s.remote);
  const statuses = useWorkspace((s) => s.engines.data);
  const [departure, setDeparture] = useState<NavigationResume | null>(null);
  const editButton = useRef<HTMLButtonElement>(null), lastField = useRef<HTMLElement | null>(null);
  const disposeGuard = useRef<NavigationGuardOwner | null>(null), focusEngine = useRef<string | null>(null);
  const restoreEditFocus = useRef(false);
  const alive = useRef(true);
  const refreshGeneration = useRef(0);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  const busy = !!editor && tierBusy(editor), dirty = !!editor && !sameTiers(editor.base.assignments, editor.draft);
  useEffect(() => {
    if (!read.data) return;
    const old = useTierEditors.getState().editors[scope];
    if (!old) putTierEditor(scope, initialTierEditor(read.data));
    else if (!old.editing && !tierBusy(old) && read.data.settings.revision >= old.base.revision) {
      const keepSaved = old.phase === "saved" && read.data.settings.revision === old.base.revision && sameTiers(old.base.assignments, read.data.settings.assignments);
      putTierEditor(scope, { ...initialTierEditor(read.data), phase: keepSaved ? "saved" : "idle" });
    }
  }, [scope, read.data]);
  useLayoutEffect(() => {
    if (!editor?.editing || (!dirty && !busy)) return;
    const original = window.location.href, historyState: unknown = window.history.state;
    const ask = (resume: NavigationResume) => { if (!busy) setDeparture(() => resume); };
    const release = registerNavigationGuard(ask, window.location.hash); disposeGuard.current = release;
    const click = (event: MouseEvent) => {
      const anchor = event.target instanceof Element ? event.target.closest<HTMLAnchorElement>("a[href]") : null;
      if (!anchor || event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || !anchor.getAttribute("href")?.startsWith("#")) return;
      const hash = anchor.getAttribute("href")!;
      if (hash === window.location.hash) return;
      event.preventDefault(); event.stopPropagation(); deferNavigation(() => goToHash(hash.slice(1)));
    };
    const hash = (event: HashChangeEvent) => {
      // Once Discard has released the guard the move is the person's: a synchronous router event must not be put back.
      if (window.location.href === original || !navigationGuarded()) return;
      const next = window.location.hash;
      window.history.replaceState(historyState, "", original); event.stopImmediatePropagation();
      deferNavigation(() => goToHash(next.slice(1)));
    };
    const unload = (event: BeforeUnloadEvent) => { event.preventDefault(); event.returnValue = ""; };
    document.addEventListener("click", click, true); window.addEventListener("hashchange", hash, true); window.addEventListener("beforeunload", unload);
    return () => { release.dispose(); if (disposeGuard.current === release) disposeGuard.current = null; document.removeEventListener("click", click, true); window.removeEventListener("hashchange", hash, true); window.removeEventListener("beforeunload", unload); };
  }, [editor?.editing, dirty, busy]);
  useLayoutEffect(() => {
    if (editor?.editing && focusEngine.current) { document.getElementById(`crew-tier-${focusEngine.current}`)?.focus(); focusEngine.current = null; }
    if (!editor?.editing && restoreEditFocus.current) { restoreEditFocus.current = false; (editButton.current?.disabled ? document.querySelector<HTMLElement>("[data-page-title]") : editButton.current)?.focus(); }
  }, [editor?.editing]);
  if (!editor) return read.status === "error" ? <ErrorState title="Can’t read engine tiers" failure={read.error} onRetry={() => void read.reload()} /> : <Loading label="Reading engine tiers" rows={2} />;
  const readonly = remote || !editor.view.writable;
  const cancel = () => {
    if (tierBusy(useTierEditors.getState().editors[scope]!)) return;
    refreshGeneration.current += 1;
    disposeGuard.current?.dispose(); disposeGuard.current = null;
    restoreEditFocus.current = true;
    patchTierEditor(scope, { draft: copyTiers(editor.base.assignments), editing: false, phase: "idle", error: undefined, previous: undefined });
    setDeparture(null);
  };
  const edit = (id?: string) => { if (readonly || busy) return; focusEngine.current = id ?? null; patchTierEditor(scope, { editing: true, phase: "idle", error: undefined }); };
  const refresh = async (mappings = false) => {
    const owner = useTierEditors.getState().editors[scope];
    if (!owner || tierBusy(owner)) return;
    const generation = ++refreshGeneration.current;
    // Every edit/write replaces this store entry. An older read owns only that exact entry.
    const ownsRefresh = () => alive.current && generation === refreshGeneration.current && controlScope() === scope && useTierEditors.getState().editors[scope] === owner;
    try {
      assertControlScope(scope); const view = await reviewControlApi.tiers();
      if (!ownsRefresh() || view.settings.revision < owner.base.revision) return;
      if (mappings) patchTierEditor(scope, { view, base: view.settings, draft: copyTiers(view.settings.assignments), previous: copyTiers(owner.draft), phase: "idle", error: undefined });
      else if (view.settings.revision !== owner.base.revision && owner.editing) patchTierEditor(scope, { view, phase: "conflict", error: "Engine tiers changed on this computer. Reload the current mappings before saving." });
      else if (!owner.editing) putTierEditor(scope, { ...initialTierEditor(view), phase: owner.phase === "saved" ? "saved" : "idle" });
      else patchTierEditor(scope, { view, refreshFailed: false });
    } catch { if (ownsRefresh()) patchTierEditor(scope, { error: "Couldn’t refresh engines. Your changes are still here.", refreshFailed: true }); }
  };
  const ids = [...new Set([...editor.view.engines.map((e) => e.id), ...Object.keys(editor.base.assignments), ...Object.keys(editor.draft), ...Object.keys(editor.previous ?? {})])];
  const valid = new Set(editor.view.engines.map((e) => e.id)).size === editor.view.engines.length && Object.entries(editor.draft).every(([id, row]) => !tierProblem(editor.view, id, row));
  return <section className="ap-set et-section" aria-label="Engine tiers" aria-busy={busy} onFocusCapture={(event) => { if (event.target instanceof HTMLSelectElement) lastField.current = event.target; }} onKeyDown={(event) => {
    // An open native picker consumes its own Escape before this bubbling handler.
    if (event.key === "Escape" && !event.defaultPrevented && editor.editing) { event.preventDefault(); event.stopPropagation(); if (!busy) cancel(); }
  }}>
    <header className="et-heading"><div><h3>Engine tiers</h3><small>Applies to this computer</small></div>{!editor.editing ? <button type="button" className="btn btn-secondary btn-small" ref={editButton} disabled={readonly || busy} onClick={() => edit()}>Edit tiers</button> : null}</header>
    <p>Map ACP engines explicitly. Saving pins each mapped engine’s current default model.</p>
    {ids.map((id) => {
      const engine = editor.view.engines.find((e) => e.id === id), saved = ownTier(editor.base.assignments, id), draft = ownTier(editor.draft, id);
      const chosen = editor.editing ? draft : saved;
      const problem = draft ? tierProblem(editor.view, id, draft) : null;
      const current = engine?.defaults.length === 1 ? engine.defaults[0] : null;
      const available = engine?.available && !engine.native && !!current;
      return <div className="et-row" key={id}>
        <div><b>{engine?.name ?? id}</b>{engine?.native ? <small>Uses its model-family tiers</small> : <><small>Connects over ACP</small><code>{id}</code><small>Saved model: <code>{saved?.model ?? "Not mapped"}</code></small><small>Current default: <code>{current ?? (engine?.defaults.length ? "More than one default" : "No available default")}</code></small>{chosen ? <small>Mapped by you · {tierName(chosen.tier)} · <code>{chosen.model}</code></small> : null}</>}
          {engine?.unavailable_reason ? <small>{engine.unavailable_reason}</small> : null}
          {editor.previous && ownTier(editor.previous, id) ? <small>Previous choice: {tierName(ownTier(editor.previous, id)!.tier)} · <code>{ownTier(editor.previous, id)!.model}</code></small> : null}
          {editor.editing && saved && !draft ? <small>Uses Builder until you map it.</small> : null}
          {problem && editor.editing ? <p className="et-error" id={`crew-tier-error-${id}`}>{problem}</p> : null}
        </div>
        <div className="et-side">
        {!engine?.native && editor.editing ? <div className="et-field"><label htmlFor={`crew-tier-${id}`}>Tier · {engine?.name ?? id}</label><select id={`crew-tier-${id}`} value={draft?.tier ?? "unmapped"} disabled={busy || readonly || editor.phase === "conflict"} aria-invalid={!!problem} aria-describedby={problem ? `crew-tier-error-${id}` : undefined} onChange={(event) => {
          const next = copyTiers(editor.draft), tier = event.target.value;
          if (tier === "unmapped") delete next[id]; else next[id] = { tier: tier as RouteTier, model: draft?.model ?? saved?.model ?? current ?? "" };
          patchTierEditor(scope, { draft: next, phase: "idle", error: undefined });
        }}><option value="unmapped">Builder · not mapped</option><option value="lead" disabled={!available}>Lead</option><option value="worker" disabled={!available}>Builder</option><option value="scout" disabled={!available}>Scout</option></select>
          {draft && current && draft.model !== current && engine?.available ? <Button small disabled={busy || readonly || editor.phase === "conflict"} onClick={() => patchTierEditor(scope, { draft: { ...editor.draft, [id]: { ...draft, model: current } }, error: undefined })}>Use current default</Button> : null}
        </div> : !engine?.native && !saved ? <Button small disabled={readonly || !available} onClick={() => edit(id)}>Map it</Button> : null}
          <CrewRoleNote engine={statuses?.find((status) => status.id === id) as CrewRoleFields | undefined} />
        </div>
      </div>;
    })}
    {readonly ? <p>Only a person on this computer can change engine tiers.</p> : null}
    {editor.phase === "saved" ? <p role="status">Engine tiers saved.</p> : null}
    {editor.refreshFailed && editor.phase === "saved" ? <p role="alert">Couldn’t refresh engines.</p> : null}
    {editor.error ? <p role="alert" className="et-error">{editor.error}</p> : null}
    {editor.phase === "checking" ? <><p role="status">Checking whether engine tiers were saved…</p><small>Your changes are kept while we check.</small></> : null}
    {departure ? <div className="et-departure" role="alert"><p>Discard your changes to engine tiers?</p><Button small onClick={() => { disposeGuard.current?.cancelAttempt(); setDeparture(null); lastField.current?.focus(); }}>Keep editing</Button><Button small onClick={() => { if (!alive.current || controlScope() !== scope) return; departure(cancel); }}>Discard changes</Button></div> : null}
    <div className="et-actions">
      {editor.phase === "unknown" ? <Button small onClick={() => void reconcileTiers(scope)}>Check again</Button> : editor.phase === "conflict" ? <Button small onClick={() => void refresh(true)}>Reload mappings</Button> : <>
        <Button small tone="quiet" disabled={busy} onClick={() => void refresh()}>Refresh engines</Button>
        {editor.editing ? <><Button small disabled={busy} onClick={cancel}>Cancel</Button><Button small tone="primary" disabled={busy || readonly || !dirty || !valid} onClick={() => void saveTiers(scope)}>{editor.phase === "saving" ? "Saving…" : editor.phase === "checking" ? "Checking…" : "Save tiers"}</Button></> : null}
      </>}
    </div>
  </section>;
}
