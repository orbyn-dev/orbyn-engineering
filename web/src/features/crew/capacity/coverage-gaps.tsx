import { Btn, Ic } from "../shared/parts";
import { CoverageShell } from "./coverage-shell";
import { Planned, WHY } from "./Planned";

const NOTE = "The notes ref couldn’t be updated";

/** Crew Designs v77, frame `capacity-coverage-gaps`: the banner is built; the two lists and Try the 2 again are Planned. */
export function CapacityCoverageGaps() {
  return (
    <CoverageShell project="orbyn-engineering">
      <div className="cx-cwbn" style={{ margin: "0" }}>
        <Ic n="info" sm />
        <span>
          <b>14 of 38 merges aren’t in the map.</b>
          {" They are from before Mon 12 Oct 2026, had no stored hunks or had no note written. Their lines are Not measured, not counted as read or unread."}
        </span>
        <span className="cx-acts"><Planned why={WHY.gaps} /><Btn className="cx-btn">Try the 2 again</Btn></span>
      </div>
      <div className="cx-t15list">
        <div className="cx-hd"><b>Merges with no provenance note<Planned why={WHY.gaps} /></b><span>2 merges</span></div>
        <div className="cx-t15row cx-t15nc">
          <div className="cx-m"><b>Add a retry to the calendar feed</b><small>Merged Sat 31 Oct, 10:20 · codex · GPT-5.5 · web/app</small></div>
          <span className="cx-t15tag">{NOTE}</span>
        </div>
        <div className="cx-t15row cx-t15nc">
          <div className="cx-m"><b>Update the login docs for returnTo</b><small>Merged Wed 4 Nov, 16:10 · claude · Sonnet 5.5 · docs/guides</small></div>
          <span className="cx-t15tag">{NOTE}</span>
        </div>
      </div>
      <div className="cx-t15list">
        <div className="cx-hd"><b>Before provenance began<Planned why={WHY.gaps} /></b><span>12 merges · before Mon 12 Oct 2026</span></div>
        <div className="cx-t15row cx-t15nc">
          <div className="cx-m">
            <b>Not measured</b>
            <small>Crew started recording which lines an agent wrote on Mon 12 Oct 2026. Earlier merges have no record, and Crew doesn’t guess.</small>
          </div>
          <span className="cx-t15tag">Not measured</span>
        </div>
      </div>
    </CoverageShell>
  );
}
