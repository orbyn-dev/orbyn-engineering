import type { CrewAcpTierView } from "../../../../generated/CrewAcpTierView";
import type { CrewAcpTierSettings } from "../../../../generated/CrewAcpTierSettings";
import type { CrewAcpTierAssignment } from "../../../../generated/CrewAcpTierAssignment";
import type { CrewAcpTierSave } from "../../../../generated/CrewAcpTierSave";

export type TierDraft = Record<string, CrewAcpTierAssignment>;
export const ownTier = (map: CrewAcpTierSettings["assignments"], id: string): CrewAcpTierAssignment | undefined => Object.hasOwn(map, id) ? map[id] : undefined;
export const copyTiers = (map: CrewAcpTierSettings["assignments"]): TierDraft => Object.fromEntries(Object.entries(map).map(([id, row]) => { if (!row) throw new Error("Mappings could not be read."); return [id, { ...row }]; }));
export const sameTiers = (a: CrewAcpTierSettings["assignments"], b: CrewAcpTierSettings["assignments"]): boolean => Object.keys(a).length === Object.keys(b).length && Object.entries(a).every(([id, row]) => !!row && ownTier(b, id)?.tier === row.tier && ownTier(b, id)?.model === row.model);
export function tierProblem(view: CrewAcpTierView, id: string, row: CrewAcpTierAssignment): string | null {
  const engine = view.engines.find((e) => e.id === id);
  if (!engine || engine.native || !engine.available) return `${engine?.name ?? id} is unavailable. Remove its mapping or refresh engines before saving.`;
  if (engine.defaults.length === 0) return `${engine.name} has no available default model. Refresh engines before saving.`;
  if (engine.defaults.length !== 1) return `${engine.name} reports more than one default model. Refresh engines before saving.`;
  if (engine.defaults[0] !== row.model) return "The default model changed. Review it before saving.";
  return null;
}
export function tierSave(base: CrewAcpTierSettings, draft: TierDraft, view: CrewAcpTierView): CrewAcpTierSave {
  if (new Set(view.engines.map((e) => e.id)).size !== view.engines.length) throw new Error("Refresh engines before saving.");
  for (const [id, row] of Object.entries(draft)) {
    const error = tierProblem(view, id, row); if (error) throw new Error(error);
    if (!["lead", "worker", "scout"].includes(row.tier)) throw new Error(`Choose a tier for ${id}.`);
  }
  return { seen: base.revision, assignments: Object.entries(draft).map(([engine, row]) => ({ engine, tier: row.tier, expected_default_model: row.model })) };
}
