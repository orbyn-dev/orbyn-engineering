import { Act, RiskChip } from "./Parts";
import { CrewFrame } from "./CrewFrame";

/**
 * watch-flow: the Flow graph with its two dashed after-merge loops. The graph itself belongs to the Flow
 * screens and is not drawn here; this frame keeps the task header and the legend under it.
 */
export function FlowAfterMergeScreen() {
  return (
    <CrewFrame
      tab="Flow"
      reviewCount={7}
      crumbs={
        <>
          <span className="crew-muted">All tasks</span>
          <b>Use next in the auth redirect</b>
          <RiskChip tier="medium" />
          <span className="crew-end-acts">
            <Act label="Add reviewer" />
            <Act label="Round 1" />
          </span>
        </>
      }
    >
      <div className="crew-flk">
        <span>On the lines:</span>
        <span>brief</span>
        <span>diff</span>
        <span>check results</span>
        <span>findings</span>
        <span>PR</span>
        <span>Gate decided by:</span>
        <span>code</span>
        <span>a confirmed finding</span>
        <span>you</span>
        <span>Click a line to open what it carries</span>
      </div>
    </CrewFrame>
  );
}
