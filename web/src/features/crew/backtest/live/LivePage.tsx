import type { ReactNode } from "react";
import type { Project } from "../../../../api/types";
import { NETWORK_ERROR, type ApiFailure } from "../../../../api/client";
import type { BacktestEstimate } from "../../../../generated/BacktestEstimate";
import { Loading } from "../../../../product/ui/states";
import { Planned } from "../../capacity/Planned";
import { useNarrow } from "../../board/useWidth";
import { ProjectChip } from "../../escape/live/parts";
import { Frame } from "../../shared/Frame";
import { Ic } from "../../shared/parts";
import { useCrewHref } from "../../shared/nav";
import { count } from "./model";
import { useBacktestEstimate } from "./useBacktest";

/** Why each Planned pill on this page is not built, in the words its pill gives. */
const WHY = {
  lanes: "the app has no read of the review lanes, their engines or their models",
  budgets: "the app has no read of the run budgets (#760 is pending)",
  last: "no read lists past backtests, so the app cannot name the last one",
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

function Rows({ estimate }: { estimate: BacktestEstimate }) {
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
        <Row title="Which lanes" planned={WHY.lanes} value={<Value>Not shown</Value>}>
          Every review lane in the repo’s lane rules, and the checks. The app has no read of the lanes, their engines or their models, so none is listed.
        </Row>
        <Row title="Run budget" planned={WHY.budgets} value={<Value>Not shown</Value>}>
          Each engine has a budget in its own unit, never added up. The app has no read of the budgets, so none is shown.
        </Row>
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
        <Row title="Last backtest" planned={WHY.last} value={<Value>Not shown</Value>}>
          None is shown. The app can open a backtest it is given by its id: a running one is offered in a banner at the top of this page.
        </Row>
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
 * Settings › Crew › Reviewer backtest (Crew Designs v87, `entry-crew-backtest-page` and its states), on the real estimate read. It says first
 * that the live runner is not approved and a backtest can't start, then what a backtest replays, which pull requests it may replay and
 * whether outcomes can be measured, all from the estimate. The lane list, the run budgets and the last backtest are Planned and drawn as
 * "Not shown": no read serves them, so none is invented. When the estimate names a run in progress the page offers its report instead of the
 * not-approved line. There is no start and no stop here: opening the page, a retry and the links send nothing but the estimate read.
 */
export function LivePage({ project, projects, onPick }: { project: Project; projects: readonly Project[]; onPick(id: string): void }) {
  const read = useBacktestEstimate(project.id);
  const href = useCrewHref();
  const [ref, narrow] = useNarrow();
  const estimate = read.status === "ready" ? read.data : null;
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
              <b>A backtest is running.</b> Only one runs at a time: it finishes by itself, or a person stops it on its report.
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
        {read.status === "loading" ? <Loading label="Reading the backtest estimate" rows={3} /> : null}
        {read.status === "error" ? <EstimateError failure={read.error} onRetry={() => void read.reload()} /> : null}
        {estimate ? <Rows estimate={estimate} /> : null}
      </div>
    </Frame>
  );
}
