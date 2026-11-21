import type { ReactNode } from "react";
import { barHeights, dayLong, fill, plural, type WeekRow } from "./model";

/** The "Planned" pill of the Crew designs: it marks what a screen draws that the app cannot measure yet. `why` is the reason, for everyone. */
export function Planned({ why }: { why: string }) {
  return (
    <span className="cx-cwpv" style={{ height: "16px" }} title={`Planned: ${why}`} aria-label={`Planned: ${why}`}>
      Planned
    </span>
  );
}

/** Why each figure is not drawn yet, in the words its pill gives. */
export const WHY = {
  autonomy: "the service does not record what each merge needed from you yet, so there is no autonomy rate",
  cycle: "the service does not record when a brief was given and when it merged, so there is no cycle time",
  cost: "the service does not total what each engine used per merge yet, so there is no cost",
  weeks: "the service has no merges per week yet, so there are no bars",
  period: "the service reads one window; 30 and 90 days wait for the dashboard read",
  queue: "the Review queue is not in the beta, so there is no button to open it",
  earning: "earning a higher mode for an area is not built; an area only goes down and back to your ceiling",
} as const;

/**
 * Merges per week as the design draws them: a bar's height is that week's merges, the figure under it is the same number, and the filled part is
 * the share that needed nothing from you (or was reverted). Nothing here is a percentage of a week: no week has the sample for one.
 */
export function Bars({ weeks, mode, big }: { weeks: readonly WeekRow[]; mode: "unaided" | "reverted"; big?: boolean }) {
  const heights = barHeights(weeks);
  const word = mode === "unaided" ? "needed nothing from you" : "reverted";
  const summary = weeks.map((week, at) => `week ${at + 1}: ${plural(week.merges, "merge")}, ${mode === "unaided" ? week.unaided : week.reverted} ${word}`).join("; ");
  return (
    <div className="cx-t10bars" role="img" aria-label={`Merges per week. ${summary}`} style={big ? { height: "120px" } : undefined}>
      {weeks.map((week, at) => {
        const part = mode === "unaided" ? week.unaided : week.reverted;
        return (
          <div key={`${week.start}-${at}`} title={`Week of ${dayLong(week.start)}: ${plural(week.merges, "merge")}, ${part} ${word}`}>
            <i style={{ height: `${heights[at] ?? 0}%` }}>
              <b style={{ height: `${fill(part, week.merges)}%` }} />
            </i>
            <em>{week.merges}</em>
          </div>
        );
      })}
    </div>
  );
}

/** One tile. `sample` is the line at the bottom that says what the figure rests on. */
export function Tile({ label, planned, value, sub, children, sample }: { label: string; planned?: string; value: ReactNode; sub?: ReactNode; children?: ReactNode; sample: ReactNode }) {
  return (
    <div className="cx-t10c" data-tile={label}>
      <span className="cx-t10l">
        {label}
        {planned ? <> <Planned why={planned} /></> : null}
      </span>
      <span className="cx-t10b">{value}</span>
      {sub ? <span className="cx-t10s">{sub}</span> : null}
      {children}
      <span className="cx-t10f">{sample}</span>
    </div>
  );
}
