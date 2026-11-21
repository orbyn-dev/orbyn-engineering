import type { CrewRoleFields } from "./contract";

/** The label a Crew engine list shows for an engine that may only review, or null for any other. NOT frame-checked: the Crew frames are not merged. */
export function crewRoleLabel(engine: CrewRoleFields | null | undefined): string | null {
  return engine?.crew_role === "reviewer_only" ? "Reviewer only" : null;
}

/** "Reviewer only" in small type, with the service's reason as its title and, when there is one, a line after it. */
export function CrewRoleNote({ engine }: { engine: CrewRoleFields | null | undefined }) {
  const label = crewRoleLabel(engine);
  if (!label) return null;
  const reason = typeof engine?.crew_role_reason === "string" && engine.crew_role_reason.trim() !== "" ? engine.crew_role_reason.trim() : null;
  return (
    <small className="ct-role" title={reason ?? undefined}>
      {label}
      {reason ? ` · ${reason}` : ""}
    </small>
  );
}
