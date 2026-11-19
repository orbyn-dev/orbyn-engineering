import { useState } from "react";
import type { Project } from "../../../../api/types";
import { ErrorState, Loading } from "../../../../product/ui/states";
import { Planned } from "../../capacity/Planned";
import { ProjectChip } from "../../escape/live/parts";
import { Frame } from "../../shared/Frame";
import { Btn, Ic } from "../../shared/parts";
import { count, WHY } from "./model";
import { useBacktestEstimate } from "./useBacktest";

/** The outcomes line of the estimate: measured from the escape log, or the honest Not measured when it cannot say. */
function Outcomes({ measured }: { measured: boolean }) {
  return measured ? (
    <span><b>Outcomes: Measured from the escape log.</b> Each lane is scored against the escapes it could have caught.</span>
  ) : (
    <span><b>Outcomes: Not measured.</b> This project’s escape log can’t say, so the report shows finding counts only, never a zero.</span>
  );
}

/**
 * Crew Designs v23, frame `backtest-estimate`, on the real estimate route. The route says how many merged pull requests a
 * backtest may replay, the window, whether outcomes can be measured and which run is in progress; those are shown. The
 * per-lane estimate, the token estimate per engine and the start itself are Planned: no route serves the reviewer-backtest
 * lanes, their engines and models, each lane's review-or-check kind, or the per-engine token budgets, so a start body
 * could only be invented. Reads only: no load sends a write.
 */
export function LiveEstimate({ project, projects, onPick }: { project: Project; projects: readonly Project[]; onPick(id: string): void }) {
  const read = useBacktestEstimate(project.id);
  const estimate = read.status === "ready" ? read.data : null;
  const [dialog, setDialog] = useState(true);
  const openRunning = (id: string) => {
    window.location.hash = `#/preview/crew/backtest-report?id=${encodeURIComponent(id)}`;
  };
  return (
    <Frame live height={760}>
      <div className="cx-cwpg">
        <div className="cx-flcr">
          <span>Dashboard</span>
          <Ic n="right" sm />
          <b>Reviewer backtest</b>
        </div>
        <div className="cx-t14pg">
          <div className="cx-t10hd">
            <b>Reviewer backtest</b>
            <span className="cx-rtag cx-lo">This device only</span>
            <span className="cx-t10sp">
              <ProjectChip name={project.name} projects={projects} current={project.id} onPick={onPick} />
              <Btn className="cx-ghost" reason={WHY.settings}>Settings</Btn>
              <button
                type="button"
                className="cx-btn"
                disabled={!estimate || estimate.eligible_prs === 0}
                title={!estimate ? "Reading the estimate" : estimate.eligible_prs === 0 ? "No merged pull request is old enough to replay yet" : undefined}
                aria-description={!estimate || estimate.eligible_prs === 0 ? "No merged pull request is old enough to replay yet" : undefined}
                onClick={() => setDialog(true)}
              >
                Run a backtest
              </button>
            </span>
          </div>
          {read.status === "loading" ? <Loading label="Reading the backtest estimate" rows={4} /> : null}
          {read.status === "error" ? <ErrorState title="Can’t read the estimate" failure={read.error} onRetry={() => void read.reload()} /> : null}
          {estimate ? (
            <>
              {estimate.running ? (
                <div className="cx-cwbn" style={{ margin: "0" }}>
                  <Ic n="info" sm />
                  <span>
                    <b>A backtest is running.</b> Only one runs at a time: it finishes by itself, or a person stops it on its report.
                  </span>
                  <span className="cx-acts">
                    <button type="button" className="cx-ghost" onClick={() => openRunning(estimate.running ?? "")}>Open the running backtest</button>
                  </span>
                </div>
              ) : dialog ? null : (
                <div className="cx-cwbn" style={{ margin: "0" }}>
                  <Ic n="info" sm />
                  <span>No backtest is running here. Choose Run a backtest to see what it would replay.</span>
                </div>
              )}
              <div className="cx-fbx">
                <h6>The last {estimate.history_days} days</h6>
                <p>
                  <span>
                    {estimate.eligible_prs === 0
                      ? "No merged pull requests are " + estimate.outcome_days + " days old or more yet."
                      : `${count(estimate.eligible_prs)} merged pull requests are ${estimate.outcome_days} days old or more.`}
                  </span>{" "}
                  <Outcomes measured={estimate.outcomes_measured} />
                </p>
              </div>
            </>
          ) : null}
        </div>
        {estimate && estimate.eligible_prs > 0 && dialog ? (
          <div className="cx-cwov">
            <div>
              <div className="cx-cwdlg" style={{ width: "640px", minHeight: "320px" }}>
                <h4>Run a reviewer backtest</h4>
                <p>
                  Crew would replay the review lanes and the checks on {count(estimate.eligible_prs)} merged pull requests in {project.name} (the last{" "}
                  {estimate.history_days} days, {estimate.outcome_days} days old or more). It never replays the builder, and nothing is merged, reverted
                  or changed.
                </p>
                {estimate.outcomes_measured ? null : (
                  <div className="cx-cwbn" style={{ margin: "0" }}>
                    <Ic n="info" sm />
                    <span>This project’s escape log can’t say, so the report will show finding counts only, never a zero.</span>
                  </div>
                )}
                <div className="cx-t14opt">
                  <Ic n="info" sm />
                  <span>
                    <b>The lanes and the token estimate per engine are not served yet.</b>
                    <small>
                      {" The app has no read of your lanes, their engines and models, or of the per-engine token budgets, so no estimate per lane is shown. "}
                      <Planned why={`${WHY.lanes} and ${WHY.budgets}`} />
                    </small>
                  </span>
                </div>
                <div className="cx-ra">
                  <Planned why={WHY.start} style={{ marginLeft: "0", marginRight: "auto" }} />
                  <button type="button" className="cx-ghost" onClick={() => setDialog(false)}>Cancel</button>
                  <Btn className="cx-btn" reason={`Planned: ${WHY.start}`}>Start the backtest</Btn>
                </div>
              </div>
            </div>
          </div>
        ) : null}
      </div>
    </Frame>
  );
}
