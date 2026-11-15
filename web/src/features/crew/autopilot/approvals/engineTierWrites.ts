import { create } from "zustand";
import type { CrewAcpTierView } from "../../../../generated/CrewAcpTierView";
import type { CrewAcpTierSettings } from "../../../../generated/CrewAcpTierSettings";
import { useConnection } from "../../../../product/state/connection";
import { reviewControlApi, assertControlScope } from "../../shared/reviewControlApi";
import { copyTiers, sameTiers, tierSave, type TierDraft } from "./engineTierDraft";

export type TierEditor = {
  view: CrewAcpTierView; base: CrewAcpTierSettings; draft: TierDraft; editing: boolean;
  phase: "idle" | "saving" | "checking" | "unknown" | "failed" | "conflict" | "saved";
  error?: string; previous?: TierDraft; refreshFailed?: boolean;
};
export const useTierEditors = create<{ editors: Record<string, TierEditor> }>(() => ({ editors: {} }));
export function putTierEditor(scope: string, next: TierEditor) { useTierEditors.setState((s) => ({ editors: { ...s.editors, [scope]: next } })); }
export function patchTierEditor(scope: string, change: Partial<TierEditor>) { const old = useTierEditors.getState().editors[scope]; if (old) putTierEditor(scope, { ...old, ...change }); }
export function initialTierEditor(view: CrewAcpTierView): TierEditor { return { view, base: view.settings, draft: copyTiers(view.settings.assignments), editing: false, phase: "idle" }; }
export const tierBusy = (editor: TierEditor) => ["saving", "checking", "unknown"].includes(editor.phase);

async function confirmed(scope: string, saved: CrewAcpTierSettings) {
  patchTierEditor(scope, { base: saved, draft: copyTiers(saved.assignments), editing: false, phase: "saved", error: undefined, previous: undefined, refreshFailed: false });
  try { assertControlScope(scope); const view = await reviewControlApi.tiers(); patchTierEditor(scope, { view }); }
  catch { patchTierEditor(scope, { refreshFailed: true }); }
}
export async function reconcileTiers(scope: string, intended?: TierDraft) {
  const editor = useTierEditors.getState().editors[scope]; if (!editor || editor.phase === "checking") return;
  const desired = copyTiers(intended ?? editor.draft);
  patchTierEditor(scope, { phase: "checking" });
  try {
    assertControlScope(scope); const view = await reviewControlApi.tiers();
    if (view.settings.revision > editor.base.revision && sameTiers(view.settings.assignments, desired)) return confirmed(scope, view.settings);
    if (view.settings.revision !== editor.base.revision) patchTierEditor(scope, { view, phase: "conflict", error: "Engine tiers changed on this computer. Reload the current mappings before saving." });
    else patchTierEditor(scope, { view, phase: "failed", error: "Couldn’t save engine tiers. Your changes are still here. Try again." });
  } catch { patchTierEditor(scope, { phase: "unknown", error: "Couldn’t check whether engine tiers were saved. Your changes are still here." }); }
}
export async function saveTiers(scope: string) {
  const editor = useTierEditors.getState().editors[scope];
  if (!editor || tierBusy(editor) || editor.phase === "conflict" || useConnection.getState().remote || !editor.view.writable) return;
  let body;
  try { body = tierSave(editor.base, editor.draft, editor.view); } catch (error) { patchTierEditor(scope, { error: (error as Error).message }); return; }
  if (sameTiers(editor.base.assignments, editor.draft)) return;
  const intended = copyTiers(editor.draft);
  patchTierEditor(scope, { phase: "saving", error: undefined });
  try {
    assertControlScope(scope); const saved = await reviewControlApi.saveTiers(body);
    if (saved.revision <= editor.base.revision || !sameTiers(saved.assignments, intended)) throw new Error("The returned map needs reconciliation.");
    await confirmed(scope, saved);
  } catch { await reconcileTiers(scope, intended); }
}
