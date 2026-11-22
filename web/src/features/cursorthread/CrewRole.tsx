import { Info } from "lucide-react";
import type { CrewRoleFields } from "./contract";
import { displayText } from "./refusal";

/**
 * Crew's own words when the service sends no `crew_role_reason` (frames engine-picker-crew-tiers and
 * engine-picker-crew-tiers-narrow). The service's reason is specified (docs/cursor-thread-contract.md, Crew)
 * and wins when present.
 */
export const CREW_ROLE_TOOLTIP = "Orbyn cannot ask before it acts";

/** The label a Crew engine list shows for an engine that may only review, or null for any other. */
export function crewRoleLabel(engine: CrewRoleFields | null | undefined): string | null {
  return engine?.crew_role === "reviewer_only" ? "Reviewer only" : null;
}

/** The "Reviewer only" chip under the tier, with Crew's reason on hover or focus. It is a limit, not a tier and not a control. */
export function CrewRoleNote({ engine }: { engine: CrewRoleFields | null | undefined }) {
  const label = crewRoleLabel(engine);
  if (!label) return null;
  const tooltip = displayText(engine?.crew_role_reason) ?? CREW_ROLE_TOOLTIP;
  return (
    <span className="ct-role" title={tooltip} tabIndex={0}>
      <Info size={12} aria-hidden />
      {label}
    </span>
  );
}
