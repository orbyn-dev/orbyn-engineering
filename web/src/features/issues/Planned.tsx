import type { ReactNode } from "react";

/** The quiet note beside a control that is drawn but not built: it says so, and the control beside it does nothing. */
export function PlannedNote({ what }: { what: string }) {
  return <span className="iss-planned" title={`${what} is not available yet.`}>Planned</span>;
}

/**
 * A control the design draws and the beta does not build (sorting, editing the description, labels and assignees). It is a
 * disabled button, so a click sends nothing and opens nothing, with the Planned note next to it.
 */
export function PlannedControl({ label, what, className, children }: { label: string; what: string; className?: string; children: ReactNode }) {
  return (
    <span className="iss-planned-wrap">
      <button type="button" className={className ?? "iss-icb"} disabled aria-label={label} title={`${what} is not available yet.`}>{children}</button>
      <PlannedNote what={what} />
    </span>
  );
}
