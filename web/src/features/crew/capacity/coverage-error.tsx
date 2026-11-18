import { Btn, Ic } from "../shared/parts";
import { CoverageShell } from "./coverage-shell";

/** Crew Designs v77, frame `capacity-coverage-error`: one dash tile and the raw error line, never one tile per area. */
export function CapacityCoverageError() {
  return (
    <CoverageShell project="orbyn-engineering">
      <div className="cx-cwbn cx-red" style={{ margin: "0" }}>
        <Ic n="alert" sm />
        <span>Couldn’t read the provenance notes on this device. Nothing was changed, and no line was counted or removed.</span>
        <span className="cx-acts"><Btn className="cx-ghost">Copy the error</Btn><Btn className="cx-btn">Try again</Btn></span>
      </div>
      <small style={{ color: "var(--color-muted)" }}>Can’t reach the Orbyn service.</small>
      <div className="cx-t15tile"><b className="cx-n">—</b><small>Not shown while the notes can’t be read</small></div>
    </CoverageShell>
  );
}
