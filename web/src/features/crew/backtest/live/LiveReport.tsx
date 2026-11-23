import type { Project } from "../../../../api/types";
import { NETWORK_ERROR, type ApiFailure } from "../../../../api/client";
import type { BacktestRun } from "../../../../generated/BacktestRun";
import { Loading } from "../../../../product/ui/states";
import { Planned } from "../../capacity/Planned";
import { ProjectChip } from "../../escape/live/parts";
import { Frame } from "../../shared/Frame";
import { useCrewHref } from "../../shared/nav";
import { Btn, Ic } from "../../shared/parts";
import {
  count,
  escapesText,
  findingsText,
  laneStateText,
  laneTitle,
  matchedText,
  outcomesSummary,
  runDate,
  runStateText,
  scoredLanes,
  stopReasonText,
  tokens,
  usedByEngine,
  WHY,
} from "./model";
import { useBacktestReport } from "./useBacktest";
import "./backtest-live.css";

/** The read failed: nothing is shown, the exact answer is kept, and a retry reads again. */
function ReportError({ failure, onRetry }: { failure: ApiFailure; onRetry: () => void }) {
  const detail = failure.code === NETWORK_ERROR ? failure.message : `${failure.code}: ${failure.message}`;
  return (
    <>
      <div className="cx-cwbn cx-red" style={{ margin: "0" }} role="alert">
        <Ic n="alert" sm />
        <span>Can’t read the backtest. Nothing is shown, and nothing was changed.</span>
        <span className="cx-acts">
          <button type="button" className="cx-btn" onClick={onRetry}>Try again</button>
        </span>
      </div>
      <small style={{ color: "var(--color-muted)" }}>{detail}</small>
    </>
  );
}

/** Why the run ended, in the service's own reason, shown where the person sees the partial result. */
function Stopped({ run }: { run: BacktestRun }) {
  const byPerson = run.stop_reason === "stopped by the person";
  return (
    <div className="cx-cwbn" style={{ margin: "0" }}>
      <Ic n="info" sm />
      <span>
        <b>{byPerson ? "You stopped this backtest." : "This backtest stopped."}</b>
        {" "}
        {byPerson ? "" : `${stopReasonText(run.stop_reason)} `}
        The counts below are for the pull requests it replayed and no others.
      </span>
    </div>
  );
}

/**
 * Crew Designs v23, frames `backtest-report` and `backtest-stopped`, on the real report route. A run and each lane's state,
 * tokens, findings and score come from the read; a lane the escape log cannot score reads Not measured, never a zero, and a
 * stopped run reads Partial. Run again is Planned: the report serves each lane's name, engine and model and the run's
 * per-engine budgets, but not whether that lane replayed a review or a check, so a start body would have to guess that
 * field. Reads only: no load sends a write. Below its narrow breakpoint the run summary and the lane cells stack, and each
 * stacked lane cell keeps the label its column header carried.
 */
export function LiveReport({ id, projects, onPick }: { id: string; projects: readonly Project[]; onPick(id: string): void }) {
  const href = useCrewHref();
  const read = useBacktestReport(id);
  const run = read.status === "ready" ? read.data : null;
  const projectName = run ? (projects.find((entry) => entry.id === run.project)?.name ?? run.project) : null;
  const outcomes = run ? outcomesSummary(run.lanes) : null;
  const used = run ? usedByEngine(run.lanes) : [];
  return (
    <Frame live>
      <div className="cx-cwpg">
        <div className="cx-flcr">
          <span>Dashboard</span>
          <Ic n="right" sm />
          <b>Reviewer backtest</b>
        </div>
        <div className="cx-t14pg cx-bt-report">
          <div className="cx-t10hd">
            <b>Reviewer backtest</b>
            <span className="cx-rtag cx-lo">This device only</span>
            {run ? <span className="cx-apchip">{runStateText[run.state]}</span> : null}
            {run ? <span className="cx-t10f" style={{ margin: "0", fontSize: "11.5px" }}>{runDate(run)}</span> : null}
            <span className="cx-t10sp">
              {run && projectName ? <ProjectChip name={projectName} projects={projects} current={run.project} onPick={onPick} /> : null}
              <Planned why={WHY.runAgain} style={{ marginLeft: "0", marginRight: "4px" }} />
              <a className="cx-ghost" href={href({ page: "backtest" })}>Settings</a>
              <Btn className="cx-btn" reason={`Planned: ${WHY.runAgain}`}>Run again</Btn>
            </span>
          </div>
          {!id ? (
            <div className="cx-cwstate" style={{ margin: "48px auto 0" }}>
              <b>No backtest to show</b>
              <p>A report opens from a backtest that has run. Open this frame with the run’s id in the hash (<code>?id=…</code>).</p>
            </div>
          ) : null}
          {id && read.status === "loading" ? <Loading label="Reading the backtest" rows={4} /> : null}
          {id && read.status === "error" ? <ReportError failure={read.error} onRetry={() => void read.reload()} /> : null}
          {run ? (
            <>
              {run.state === "stopped" ? <Stopped run={run} /> : null}
              {run.state === "failed" ? (
                <div className="cx-cwbn cx-red" style={{ margin: "0" }} role="alert">
                  <Ic n="alert" sm />
                  <span>
                    <b>This backtest failed.</b> {stopReasonText(run.stop_reason)} The counts below are what it recorded before it stopped.
                  </span>
                </div>
              ) : null}
              {run.state === "running" ? (
                <div className="cx-cwbn" style={{ margin: "0" }}>
                  <Ic n="info" sm />
                  <span><b>This backtest is running.</b> The counts below are progress; a result is not shown until it has finished.</span>
                </div>
              ) : null}
              <div className="cx-t10g" style={{ gridTemplateColumns: "repeat(3,minmax(0,1fr))" }}>
                <div className="cx-t10c">
                  <span className="cx-t10l">Outcomes</span>
                  <span className="cx-t10b">{outcomes?.value}</span>
                  <span className="cx-t10s">{outcomes?.note}</span>
                </div>
                <div className="cx-t10c">
                  <span className="cx-t10l">Lanes that scored</span>
                  <span className="cx-t10b">{count(scoredLanes(run.lanes))} of {count(run.lanes.length)}</span>
                  <span className="cx-t10f">Each lane’s own sample decides whether a rate shows</span>
                </div>
                <div className="cx-t10c">
                  <span className="cx-t10l">What it used</span>
                  <span className="cx-t10b">Per engine</span>
                  <span className="cx-t10s">each in its own unit, never added up</span>
                  <div className="cx-t10e">
                    {used.length === 0 ? <span><i>No lane</i><b>No tokens</b></span> : used.map((entry) => <span key={entry.engine}><i>{entry.engine}</i><b>{tokens(entry.tokens)}</b></span>)}
                  </div>
                </div>
              </div>
              <h5 className="cx-t10h5">
                By lane <span className="cx-t10r">{count(run.lanes.length)} in all</span>
              </h5>
              <div className="cx-aptb" style={{ gridTemplateColumns: "minmax(0,1.6fr) minmax(0,1.1fr) minmax(0,.8fr) minmax(0,1.2fr)" }}>
                <div className="cx-h">Lane</div>
                <div className="cx-h">Escapes it would have caught</div>
                <div className="cx-h">Findings raised</div>
                <div className="cx-h">Findings that matched an escape</div>
                {run.lanes.flatMap((lane) => {
                  const escapes = escapesText(lane);
                  const findings = findingsText(lane);
                  const matched = matchedText(lane);
                  return [
                    <div key={`${lane.lane}:${lane.engine}:lane`}>
                      <b>{laneTitle(lane)}</b>
                      <small>{laneStateText[lane.state]}{lane.state === "stopped" ? " · partial" : ""}</small>
                    </div>,
                    <div key={`${lane.lane}:${lane.engine}:escapes`}>
                      <b className="cx-bt-wide">{escapes.value}</b>
                      <small className="cx-bt-wide">{escapes.note}</small>
                      <small className="cx-bt-narrow">Escapes caught: {escapes.value}{escapes.note ? ` · ${escapes.note}` : ""}</small>
                    </div>,
                    <div key={`${lane.lane}:${lane.engine}:findings`}>
                      <b className="cx-bt-wide">{findings.value}</b>
                      {findings.note ? <small className="cx-bt-wide">{findings.note}</small> : null}
                      <small className="cx-bt-narrow">Findings raised: {findings.value}{findings.note ? ` · ${findings.note}` : ""}</small>
                    </div>,
                    <div key={`${lane.lane}:${lane.engine}:matched`}>
                      <b className="cx-bt-wide">{matched.value}</b>
                      <small className="cx-bt-wide">{matched.note}</small>
                      <small className="cx-bt-narrow">Matched an escape: {matched.value}{matched.note ? ` · ${matched.note}` : ""}</small>
                    </div>,
                  ];
                })}
              </div>
              <p className="cx-t10f" style={{ margin: "6px 0 0", display: "flex", alignItems: "center", gap: "6px" }}>
                <Planned why={WHY.replays} style={{ marginLeft: "0" }} />
                <span>The report route does not serve how many pull requests each lane replayed.</span>
              </p>
              <div className="cx-t14two">
                <div className="cx-fbx">
                  <h6>How to read the columns</h6>
                  <p>
                    Escapes it would have caught: of the escapes in the window, how many this lane flagged when replayed. A rate shows from 5
                    escapes. Findings that matched an escape: of the findings the lane raised, how many pointed at a change that was later
                    reverted or fixed. A rate shows from 10 findings. A finding that matched no escape is unmatched, never wrong.
                  </p>
                </div>
                <div className="cx-fbx">
                  <h6>What this tells you, and what it doesn’t</h6>
                  <p>
                    It replays the review lanes and the checks on pull requests that already merged, as they were at merge time, and compares them
                    with what happened next. It never replays the builder: building isn’t repeatable, so a replay of it would measure nothing. A
                    model’s review can differ from one run to the next, so each replay is one sample, not a promise. Crew makes no claim that one
                    engine, or one vendor, reviews better. This is what happened on this repo, with the sample under every figure.
                  </p>
                </div>
              </div>
            </>
          ) : null}
        </div>
      </div>
    </Frame>
  );
}
