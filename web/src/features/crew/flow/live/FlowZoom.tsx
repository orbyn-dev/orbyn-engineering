import { ApiFailure } from "../../../../api/client";
import type { FlowFinding } from "../../../../generated/FlowFinding";
import type { FlowLane } from "../../../../generated/FlowLane";
import type { FlowTaskDetail } from "../../../../generated/FlowTaskDetail";
import { ErrorState, Empty, Loading } from "../../../../product/ui/states";
import { Button } from "../../../../ui/controls";
import { DownIcon, RightIcon, StageIcon } from "./glyphs";
import { cellOf, decidedBy, laneWord, plain, roundView, STAGES, stageWord, when } from "./model";
import { useFlowTask, useRefresh } from "./useFlow";

/** Why the two write buttons are off: their routes exist, but this view does not call them yet. */
const LATER = "Available when the write route ships";
/** The Review queue is not a tab of the Crew view yet, so Review has nowhere to open. */
const NO_QUEUE = "The Review queue is not in this app yet";

/**
 * One task zoomed, on the Flow task route: the ten stages as a list with the reviewer lanes under their stage, then the
 * brief, the route, the findings, the gates, the rounds, the merge, main and the person's part. No check, command, output
 * or "proves" text is in the answer, so none is drawn. A gate's reason is present only once the task is final.
 */
export function FlowZoom({ id, onBack }: { id: string; onBack(): void }) {
  const read = useFlowTask(id);
  const running = read.data ? !["passed", "stopped", "cancelled"].includes(read.data.row.state) : false;
  useRefresh(read.reload, !running);
  const back = (
    <Button small onClick={onBack}>
      All tasks
    </Button>
  );
  if (read.status === "loading") return <Loading label="Reading the task" rows={4} />;
  if (read.status === "error" && !read.data) {
    const missing = read.error instanceof ApiFailure && read.error.status === 404;
    return (
      <div className="fv-body">
        <div className="fv-crumb">{back}</div>
        {missing ? <Empty title="This task isn't in Flow" /> : <ErrorState title="Can't read this task" failure={read.error} onRetry={() => void read.reload()} />}
      </div>
    );
  }
  return <Detail detail={read.data!} back={back} />;
}

function Detail({ detail, back }: { detail: FlowTaskDetail; back: React.ReactNode }) {
  const { row } = detail;
  const round = roundView(row);
  const final = ["passed", "stopped", "cancelled"].includes(row.state);
  return (
    <div className="fv-body fv-zoom">
      <div className="fv-crumb">
        {back}
        <h2>{row.name || "Untitled task"}</h2>
        <span className="fv-badge">{round.text}</span>
        <span className="fv-badge">{plain(row.state)}</span>
        <span className="fv-spacer" />
        <button type="button" className="fv-off" disabled aria-description={LATER} title={LATER}>
          Cancel
        </button>
        <button type="button" className="fv-off" disabled aria-description={LATER} title={LATER}>
          Re-run checks
        </button>
      </div>
      <p className="fv-later">Cancel and Re-run checks: {LATER}</p>
      {detail.truncated ? (
        <p className="fv-note" role="note">
          A list of this task was cut at its cap; the newest are shown.
        </p>
      ) : null}

      <div className="fv-stages" role="list" aria-label="Stages">
        {STAGES.map((one) => {
          const cell = cellOf(row, one.stage);
          const word = stageWord(one.stage, cell);
          const lanes = detail.lanes.filter((lane) => (one.stage === "code" ? lane.group === "code" : one.stage === "security" ? lane.group === "security" : false));
          return (
            <div key={one.stage} role="listitem">
              <div className="fv-stage">
                {lanes.length > 0 ? <DownIcon /> : <RightIcon />}
                <StageIcon stage={one.stage} />
                <b>{one.name}</b>
                <span className={`fv-st fv-st-${word.tone}`}>{word.text}</span>
              </div>
              {lanes.length > 0 ? (
                <ul className="fv-lanes">
                  {lanes.map((lane) => (
                    <LaneRow key={`${lane.lane}-${lane.task}`} lane={lane} />
                  ))}
                </ul>
              ) : null}
            </div>
          );
        })}
      </div>

      <div className="fv-two">
        <section className="fv-box" aria-label="Brief">
          <h3>Brief</h3>
          <dl>
            <dt>Purpose</dt>
            <dd>{detail.brief.purpose}</dd>
            <dt>Files</dt>
            <dd>{detail.brief.files.length === 0 ? "None" : detail.brief.files.join(", ")}</dd>
            <dt>Budget</dt>
            <dd>
              {detail.brief.budget_turns} turns · {detail.brief.budget_minutes} min
            </dd>
          </dl>
        </section>
        <section className="fv-box" aria-label="Route">
          <h3>Route</h3>
          {detail.route ? (
            <dl>
              <dt>Engine</dt>
              <dd>{detail.route.engine}</dd>
              <dt>Model</dt>
              <dd>{detail.route.model}</dd>
              <dt>Tier</dt>
              <dd>{detail.route.tier}</dd>
            </dl>
          ) : (
            <p className="fv-muted">Not routed yet.</p>
          )}
        </section>
      </div>

      <section className="fv-sec" aria-label="Findings">
        <h3>Findings</h3>
        {detail.findings.length === 0 ? (
          <p className="fv-muted">No findings.</p>
        ) : (
          <ul className="fv-finds">
            {detail.findings.map((finding) => (
              <Finding key={finding.id} finding={finding} />
            ))}
          </ul>
        )}
      </section>

      <section className="fv-sec" aria-label="Gates">
        <h3>Gates</h3>
        {detail.gates.length === 0 ? (
          <p className="fv-muted">No gate has been decided yet.</p>
        ) : (
          <ul className="fv-gates">
            {detail.gates.map((gate, at) => (
              <li key={`${gate.stage}-${at}`}>
                <b>{gate.stage}</b>
                <span className="fv-badge">{plain(gate.decision)}</span>
                <span className="fv-muted">Decided by {decidedBy(gate)}</span>
                <span className="fv-muted">{when(gate.at)}</span>
                {final && gate.reason ? <span className="fv-reason">{gate.reason}</span> : null}
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="fv-sec" aria-label="Rounds">
        <h3>Rounds</h3>
        <ol className="fv-rounds">
          {detail.rounds.map((one) => (
            <li key={one.n} className="fv-badge">
              Round {one.n} · {plain(one.state)}
            </li>
          ))}
        </ol>
      </section>

      <div className="fv-two">
        <section className="fv-box" aria-label="Merge and main">
          <h3>Merge and main</h3>
          {detail.merge ? (
            <dl>
              <dt>Merge</dt>
              <dd>{plain(detail.merge.state)}</dd>
              <dt>Risk</dt>
              <dd>{detail.merge.risk}</dd>
              <dt>Mode</dt>
              <dd>{detail.merge.mode}</dd>
              {detail.merge.reasons.length > 0 ? (
                <>
                  <dt>Why</dt>
                  <dd>{detail.merge.reasons.join(" · ")}</dd>
                </>
              ) : null}
              <dt>Main</dt>
              <dd>{detail.watch ? plain(detail.watch) : "Not merged"}</dd>
            </dl>
          ) : (
            <p className="fv-muted">Not merged yet.</p>
          )}
        </section>
        <section className="fv-box" aria-label="You">
          <h3>You</h3>
          {detail.you ? (
            <>
              <dl>
                <dt>Review</dt>
                <dd>{plain(detail.you.review_state)}</dd>
                {detail.you.due_unix != null ? (
                  <>
                    <dt>Review by</dt>
                    <dd>{when(new Date(detail.you.due_unix * 1000).toISOString())}</dd>
                  </>
                ) : null}
                {detail.you.needs_person ? (
                  <>
                    <dt>Needs you</dt>
                    <dd>{detail.you.needs_person}</dd>
                  </>
                ) : null}
              </dl>
              {detail.you.review_state === "waiting" ? (
                <button type="button" className="fv-off" disabled aria-description={NO_QUEUE} title={NO_QUEUE}>
                  Review
                </button>
              ) : null}
            </>
          ) : (
            <p className="fv-muted">Nothing to review yet.</p>
          )}
        </section>
      </div>
    </div>
  );
}

function LaneRow({ lane }: { lane: FlowLane }) {
  return (
    <li>
      <span>
        {lane.lane}
        {lane.engine ? ` · ${lane.engine}` : ""}
        {lane.model ? ` · ${lane.model}` : ""}
      </span>
      <span className="fv-n">{laneWord(lane)}</span>
    </li>
  );
}

function Finding({ finding }: { finding: FlowFinding }) {
  return (
    <li className="fv-find">
      <div className="fv-find-top">
        <span className="fv-badge">{finding.severity}</span>
        <b>{finding.claim}</b>
        <span className="fv-badge">{plain(finding.check)}</span>
      </div>
      <code className="fv-path">
        {finding.path}
        {finding.line > 0 ? `:${finding.line}` : ""}
      </code>
      <small className="fv-muted">
        {finding.lane} · round {finding.round} · {plain(finding.status)}
        {finding.blocks ? " · blocks" : ""}
      </small>
    </li>
  );
}
