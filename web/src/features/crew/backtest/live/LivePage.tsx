import type { ReactNode } from "react";
import type { Project } from "../../../../api/types";
import { NETWORK_ERROR, type ApiFailure } from "../../../../api/client";
import type { BacktestEstimate } from "../../../../generated/BacktestEstimate";
import type { BacktestStartView } from "../../../../generated/BacktestStartView";
import { useWaited } from "../../../../lib/motion";
import type { AsyncState } from "../../../../lib/useAsync";
import { Loading } from "../../../../product/ui/states";
import { Planned } from "../../capacity/Planned";
import { useNarrow } from "../../board/useWidth";
import { ProjectChip } from "../../escape/live/parts";
import { Frame } from "../../shared/Frame";
import { Ic } from "../../shared/parts";
import { useCrewHref } from "../../shared/nav";
import { count, startBudgetEntries, startLaneKindText, startLaneTitle, startStateText, tokens } from "./model";
import { useBacktestEstimate, useBacktestStartView } from "./useBacktest";

/** Why choosing lanes or a budget before a first backtest is still Planned, in the words each pill gives. */
const WHY_FIRST = {
  lanes: "choosing lanes before a first backtest. The service stores a lane set only with a run, and no settings write for lanes is designed yet",
  budgets: "setting a budget before a first backtest. The service stores budgets only with a run, and no route sets or edits one yet",
} as const;

const NO_HISTORY = "No merged pull request is old enough to replay yet";

/** One row: the title (with its Planned pill), what it says, and its value or button. */
function Row({ title, planned, children, value }: { title: string; planned?: string; children: ReactNode; value: ReactNode }) {
  return (
    <div className="cx-setrow">
      <div>
        <b>
          {title}
          {planned ? <Planned why={planned} style={{ marginLeft: "6px" }} /> : null}
        </b>
        <small>{children}</small>
      </div>
      {value}
    </div>
  );
}

const Value = ({ children }: { children: ReactNode }) => <span className="cx-sv cx-ro">{children}</span>;

/** A read-only list inside a row: the left side shortens, the right side keeps its word. */
function LineList({ children }: { children: ReactNode }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "3px", width: "320px", maxWidth: "100%", fontSize: "12px", color: "var(--color-textSoft)" }}>
      {children}
    </div>
  );
}

function Line({ label, value, quiet }: { label: string; value: string; quiet?: boolean }) {
  return (
    <span style={{ display: "flex", gap: "10px", justifyContent: "space-between", minWidth: 0 }}>
      <i style={{ fontStyle: "normal", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", minWidth: 0 }}>{label}</i>
      <b style={{ whiteSpace: "nowrap", flex: "none", fontWeight: quiet ? 400 : 500, color: quiet ? "var(--color-muted)" : "var(--color-text)" }}>{value}</b>
    </span>
  );
}

/** The quiet bars the design draws only after 400 ms; the status is read out at once. */
function Bars({ widths }: { widths: readonly [string, string] }) {
  return (
    <div aria-hidden="true" style={{ display: "flex", flexDirection: "column", gap: "8px", padding: "4px 0", width: "320px", maxWidth: "100%" }}>
      {widths.map((width) => (
        <span key={width} data-skeleton style={{ display: "block", height: "14px", width, borderRadius: "6px", background: "var(--color-sunken)" }} />
      ))}
    </div>
  );
}

function ReadingRows({ after }: { after: ReactNode }) {
  const slow = useWaited(true);
  return (
    <>
      <span className="sr-only" role="status" aria-label="Reading the lanes and budgets">Reading the lanes and budgets</span>
      <Row title="Which lanes" value={slow ? <Bars widths={["70%", "52%"]} /> : null}>
        Reading the lanes of the last backtest.
      </Row>
      <Row title="Run budget" value={slow ? <Bars widths={["46%", "60%"]} /> : null}>
        Reading the budget of each engine.
      </Row>
      <Row title="Last backtest" value={slow ? <Bars widths={["64%", "38%"]} /> : null}>
        Reading the last backtest.
      </Row>
      {after}
    </>
  );
}

/** The start input failed: the lanes, budgets and last backtest give way to one notice; the estimate rows stay. */
function StartViewError({ failure, onRetry }: { failure: ApiFailure; onRetry: () => void }) {
  const detail = failure.code === NETWORK_ERROR ? failure.message : `${failure.code}: ${failure.message}`;
  const copy = () => {
    try {
      void navigator.clipboard?.writeText(detail).catch(() => undefined);
    } catch {
      /* no clipboard here: the text stays visible below */
    }
  };
  return (
    <div className="cx-setrow">
      <div style={{ display: "flex", flexDirection: "column", gap: "6px", gridColumn: "1/-1" }}>
        <div className="cx-cwbn cx-red" style={{ margin: "0" }} role="alert">
          <Ic n="alert" sm />
          <span>Can’t read the lanes and budgets. The last backtest’s lanes and budgets can’t be read, so none is shown and the last backtest can’t be named. Nothing was changed and no backtest was started.</span>
          <span className="cx-acts">
            <button type="button" className="cx-ghost" onClick={copy}>Copy the error</button>
            <button type="button" className="cx-btn" onClick={onRetry}>Try again</button>
          </span>
        </div>
        <small style={{ color: "var(--color-muted)" }}>{detail}</small>
      </div>
    </div>
  );
}

/** The lanes, budgets and last backtest read from start-view. `after` keeps the estimate row between them, as designed. */
function StartViewRows({ read, onRetry, after }: { read: AsyncState<BacktestStartView>; onRetry: () => void; after: ReactNode }) {
  const href = useCrewHref();
  if (read.status === "loading") return <ReadingRows after={after} />;
  if (read.status === "error") {
    return (
      <>
        <StartViewError failure={read.error} onRetry={onRetry} />
        {after}
      </>
    );
  }
  const run = read.data.run;
  if (!run) {
    return (
      <>
        <Row title="Which lanes" planned={WHY_FIRST.lanes} value={<Value>Not set</Value>}>
          No backtest has run in this project, so no lane is stored, and the service doesn’t guess a set. Choosing lanes before a first backtest is not built.
        </Row>
        <Row title="Run budget" planned={WHY_FIRST.budgets} value={<Value>Not set</Value>}>
          No backtest has run in this project, so no budget is stored. Each engine will have its own, in its own unit and never added up. Setting one before a first backtest is not built.
        </Row>
        {after}
        <Row title="Last backtest" value={<Value>None yet</Value>}>
          No backtest has run in this project yet.
        </Row>
      </>
    );
  }
  const missingKind = run.lanes.some((lane) => lane.kind === null);
  const budgets = startBudgetEntries(run);
  return (
    <>
      <Row
        title="Which lanes"
        value={
          run.lanes.length === 0 ? (
            <Value>Not set</Value>
          ) : (
            <LineList>
              {run.lanes.map((lane, index) => (
                <Line key={`${lane.lane}:${lane.engine}:${index}`} label={startLaneTitle(lane)} value={startLaneKindText(lane.kind)} quiet={lane.kind === null} />
              ))}
            </LineList>
          )
        }
      >
        The lanes the last backtest ran, as it recorded them: lane, engine, model and kind. They are read here, not set: no route sets them yet. The engine and the model show by their ids, because no display name is served.
        {missingKind ? " A lane recorded before kinds were kept reads Kind not recorded, never Review." : ""}
      </Row>
      <Row
        title="Run budget"
        value={
          budgets.length === 0 ? (
            <Value>Not set</Value>
          ) : (
            <LineList>
              {budgets.map((entry) => (
                <Line key={entry.engine} label={`${entry.engine}: `} value={entry.tokens === null ? "Not set" : tokens(entry.tokens)} quiet={entry.tokens === null} />
              ))}
            </LineList>
          )
        }
      >
        The token budget each engine had in the last backtest, in its own unit and never added up. It is read here, not set: no route sets or edits a budget yet. An engine with no recorded budget reads Not set, never a number.
      </Row>
      {after}
      <Row
        title="Last backtest"
        value={
          <span style={{ display: "flex", alignItems: "center", gap: "10px", flexWrap: "wrap" }}>
            <span>{startStateText[run.state]}</span>
            <a className="cx-ghost" href={href({ page: "report", id: run.id })}>{run.state === "stopped" ? "Open the partial report" : "Open the report"}</a>
          </span>
        }
      >
        The most recent backtest, in any state. Its date, the tokens it used and the lanes it scored are in its report.
      </Row>
    </>
  );
}

function EstimateError({ failure, onRetry }: { failure: ApiFailure; onRetry: () => void }) {
  const detail = failure.code === NETWORK_ERROR ? failure.message : `${failure.code}: ${failure.message}`;
  const copy = () => {
    try {
      void navigator.clipboard?.writeText(detail).catch(() => undefined);
    } catch {
      /* no clipboard here: the text stays visible below */
    }
  };
  return (
    <>
      <div className="cx-cwbn cx-red" style={{ margin: "0" }} role="alert">
        <Ic n="alert" sm />
        <span>Can’t read the estimate. The estimate for this project can’t be read, so nothing is shown. Nothing was changed and no backtest was started.</span>
        <span className="cx-acts">
          <button type="button" className="cx-ghost" onClick={copy}>Copy the error</button>
          <button type="button" className="cx-btn" onClick={onRetry}>Try again</button>
        </span>
      </div>
      <small style={{ color: "var(--color-muted)" }}>{detail}</small>
    </>
  );
}

function Rows({ estimate, startView, onStartViewRetry }: { estimate: BacktestEstimate; startView: AsyncState<BacktestStartView>; onStartViewRetry: () => void }) {
  const href = useCrewHref();
  const none = estimate.eligible_prs === 0;
  return (
    <>
      <div>
        <Row title="What it replays" value={<Value>Reviews and checks</Value>}>
          The review lanes and the repo’s checks, on merged pull requests whose outcome is known. Never the builder: building isn’t repeatable, so a replay of it would measure nothing.
        </Row>
        <Row title="Which pull requests" value={<Value>{none ? "None yet" : `${count(estimate.eligible_prs)} pull requests`}</Value>}>
          {none ? `No merged pull requests are ${estimate.outcome_days} days old or more yet. ` : ""}
          Merged pull requests from the last {estimate.history_days} days that are {estimate.outcome_days} days old or more, so a revert or a fix has had time to happen. A pull request counts as escaped only when its escape is confirmed; a revert alone is a possible escape and is not counted.
        </Row>
        <Row title="Outcomes" value={<Value>{estimate.outcomes_measured ? "Measured" : "Not measured"}</Value>}>
          {estimate.outcomes_measured
            ? "Each lane is scored against the escapes it could have caught, from this project’s escape log."
            : "This project’s escape log can’t say, so the report shows finding counts only, never a zero."}
        </Row>
        <StartViewRows
          read={startView}
          onRetry={onStartViewRetry}
          after={
            <Row
              title="Estimate"
              value={
                none ? (
                  <button type="button" className="cx-ghost" disabled title={NO_HISTORY} aria-description={NO_HISTORY}>Open the estimate</button>
                ) : (
                  <a className="cx-ghost" href={href({ page: "estimate" })}>Open the estimate</a>
                )
              }
            >
              What a backtest would replay, before anything starts. Reads only: nothing runs and nothing is stored.
            </Row>
          }
        />
      </div>
      <div className="cx-fbx">
        <h6>What this tells you, and what it doesn’t</h6>
        <p>
          It replays the review lanes and the checks on pull requests that already merged, as they were at merge time, and compares them with what happened next. It never replays the builder: building isn’t repeatable, so a replay of it would measure nothing. A model’s review can differ from one run to the next, so each replay is one sample, not a promise. Crew makes no claim that one engine, or one vendor, reviews better. This is what happened on this repo, with the sample under every figure.
        </p>
      </div>
    </>
  );
}

/**
 * Settings › Crew › Reviewer backtest (Crew Designs v87, `entry-crew-backtest-page` and its states), on the estimate and
 * start-view reads. It says first that the live runner is not approved and a backtest can't start, then what a backtest
 * replays, which pull requests it may replay and whether outcomes can be measured, all from the estimate. The last run's
 * lanes with their kinds, its per-engine budgets and its state come from start-view; a project that never ran one reads
 * Not set with no invented lane, and a lane recorded before kinds were kept reads Kind not recorded, never Review. There
 * is no start and no stop here: opening the page, a retry and the links send nothing but reads.
 */
export function LivePage({ project, projects, onPick }: { project: Project; projects: readonly Project[]; onPick(id: string): void }) {
  const estimateRead = useBacktestEstimate(project.id);
  const startViewRead = useBacktestStartView(project.id);
  const href = useCrewHref();
  const [ref, narrow] = useNarrow();
  const estimate = estimateRead.status === "ready" ? estimateRead.data : null;
  return (
    <Frame sheet live>
      <div ref={ref} className={`cx-cwsheet${narrow ? " cx-narrow" : ""}`} style={{ maxWidth: "760px" }}>
        <h3 style={{ margin: "0", fontSize: "16px", fontWeight: 500, color: "var(--color-text)" }}>
          Crew › Reviewer backtest
          <span className="cx-cwpv">Preview</span>
        </h3>
        <div style={{ display: "flex" }}>
          <ProjectChip name={project.name} projects={projects} current={project.id} onPick={onPick} />
        </div>
        <p style={{ margin: "0", color: "var(--color-muted)", fontSize: "12.5px", lineHeight: 1.5 }}>
          Which of your review lanes, and which engines, would have caught the changes that later turned out wrong in this repo? A backtest replays them on your own history and shows the counts.
        </p>
        {estimate?.running ? (
          <div className="cx-cwbn" style={{ margin: "0" }}>
            <Ic n="info" sm />
            <span>
              <b>A backtest is running.</b> Only one runs at a time: it finishes by itself.
            </span>
            <span className="cx-acts">
              <a className="cx-ghost" href={href({ page: "report", id: estimate.running })}>Open the running backtest</a>
            </span>
          </div>
        ) : (
          <div className="cx-cwbn" style={{ margin: "0" }}>
            <Ic n="info" sm />
            <span>
              <b>The live runner is not approved.</b> A backtest can’t start: the service refuses a start until a live replay engine is connected. You can see what one would replay.
            </span>
          </div>
        )}
        {estimateRead.status === "loading" ? <Loading label="Reading the backtest estimate" rows={3} /> : null}
        {estimateRead.status === "error" ? <EstimateError failure={estimateRead.error} onRetry={() => void estimateRead.reload()} /> : null}
        {estimate ? <Rows estimate={estimate} startView={startViewRead} onStartViewRetry={() => void startViewRead.reload()} /> : null}
      </div>
    </Frame>
  );
}
