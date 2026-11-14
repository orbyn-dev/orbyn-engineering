// The Route card for a task, where a path class decided (Crew Designs frames `route-class-*`): the engine chosen, or why none was, and every
// engine considered with the service's own words for why not. Rules run before any engine is scored. Read-only: the actions that need
// a route the service lacks (open the rule, pin, split the task, change the rule, ask me to choose) are not drawn.
import type { CrewRouteClassRow } from "../../../../generated/CrewRouteClassRow";
import type { CrewRouteView } from "../../../../generated/CrewRouteView";
import type { RouteRejection } from "../../../../generated/RouteRejection";
import { navigate } from "../../../../product/state/route";
import { Button } from "../../../../ui/controls";
import { ErrorState, Loading } from "../../../../product/ui/states";
import { useAsync } from "../../../../lib/useAsync";
import { crewApi } from "./api";
import { GUARD_WORKERS } from "./copy";
import { Chip } from "./parts";
import { clockOf } from "./routing";

const FILE = ".orbyn/crew.json";

/** The frame of the designs a task's routing is in, or null when no rule took part. */
export function routeFrame(view: CrewRouteView): "route-class-blocked" | "route-class-invalid" | "route-class-none" | null {
  const row = view.rows.at(-1);
  if (!row) return null;
  if (row.problem) return "route-class-invalid";
  if (!view.decision && view.refusal) return "route-class-none";
  if (view.decision && row.blocked.length > 0) return "route-class-blocked";
  return null;
}

const rulesOf = (row: CrewRouteClassRow, engine: string) => row.blocked.filter((one) => one.engine === engine).map((one) => one.rule);

function Considered({ rejected, row, chosen }: { rejected: RouteRejection[]; row: CrewRouteClassRow; chosen?: string }) {
  return (
    <div className="ap-route-col">
      <h5>Every engine considered, and why not</h5>
      <ul className="ap-rej">
        {chosen ? <li className="ap-rej-chosen"><b>{chosen}</b><small>Chosen</small></li> : null}
        {rejected.map((one) => (
          <li key={one.engine}>
            <b>{one.engine}</b>
            <small>
              {one.text}
              {one.why === "not_signed_in" ? <> <button type="button" className="ap-link" onClick={() => navigate({ name: "settings", section: "providers" })}>Sign in</button></> : null}
            </small>
            {rulesOf(row, one.engine).map((rule) => <span key={rule} className="ap-rtag" title={`The rule ${rule} in ${FILE}`}>{rule}</span>)}
          </li>
        ))}
      </ul>
    </div>
  );
}

function Task({ row }: { row: CrewRouteClassRow }) {
  const touches = [...new Set(row.classes.flatMap((one) => one.samples))];
  return (
    <section className="ap-box">
      <h4>The task, as Crew read it</h4>
      <dl className="ap-facts">
        {touches.length ? <div><dt>Touches</dt><dd>{touches.join(", ")}</dd></div> : null}
        {row.classes.length ? <div><dt>{row.classes.length === 1 ? "Path class" : "Path classes"}</dt><dd>{row.classes.map((one) => one.name).join(" and ")}</dd></div> : null}
      </dl>
    </section>
  );
}

/** What a task's routing says when the rules of the repo took part. Pure: the screen reads the view. */
export function RouteClassCard({ view, onCheck }: { view: CrewRouteView; onCheck?(): void }) {
  const frame = routeFrame(view);
  const row = view.rows.at(-1);
  if (!frame || !row) return null;
  if (frame === "route-class-invalid") {
    return (
      <section className="ap-route" data-frame={frame} aria-label="Route">
        <div className="ap-route-head"><b>Why no engine was picked</b><Chip tone="am">Needs you</Chip></div>
        <div className="ap-box ap-box-am"><h4>{FILE} isn’t valid, so no engine was started</h4><p>Orbyn couldn’t use the file. Nothing was started. The decision is in the ledger.</p></div>
        <div className="ap-route-cols">
          <div className="ap-route-col">
            <section className="ap-box">
              <h4>The file</h4>
              <dl className="ap-facts">
                <div><dt>File</dt><dd>{FILE}</dd></div>
                <div><dt>Problem</dt><dd>{row.problem}</dd></div>
                <div><dt>Read</dt><dd>{clockOf(row.at)}</dd></div>
              </dl>
            </section>
            <section className="ap-box">
              <h4>What Orbyn does</h4>
              <p>It can’t tell which engines the rules allow, so it blocks every engine until the file is valid. It never guesses and never falls back to an engine.</p>
            </section>
          </div>
          <Considered rejected={view.refusal?.rejected ?? row.rejected} row={row} />
        </div>
        {onCheck ? <div className="ap-route-acts"><Button small onClick={onCheck}>Check again</Button></div> : null}
        <p className="ap-muted">{GUARD_WORKERS}</p>
      </section>
    );
  }
  if (frame === "route-class-none") {
    const count = row.classes.length;
    return (
      <section className="ap-route" data-frame={frame} aria-label="Route">
        <div className="ap-route-head"><b>Why no engine was picked</b><Chip tone="am">Needs you</Chip></div>
        <div className="ap-box ap-box-am">
          <h4>No engine is allowed for these paths</h4>
          <p>Crew read the task’s paths, found {count === 1 ? "one path class" : `${count} path classes`}, and no engine is allowed by {count === 2 ? "both rules" : "every rule"}. Nothing was started. The decision is in the ledger with {count === 2 ? "both rules" : "the rules"}.</p>
          {view.refusal?.message ? <p>{view.refusal.message}</p> : null}
        </div>
        <div className="ap-route-cols">
          <div className="ap-route-col"><Task row={row} /></div>
          <Considered rejected={view.refusal?.rejected ?? row.rejected} row={row} />
        </div>
        <p className="ap-muted">The task keeps its place in Needs you and nothing starts. No task runs on an engine a rule blocks.</p>
        <p className="ap-muted">{GUARD_WORKERS}</p>
      </section>
    );
  }
  const decision = view.decision;
  if (!decision) return null;
  const effort = decision.effort.charAt(0).toUpperCase() + decision.effort.slice(1);
  return (
    <section className="ap-route" data-frame={frame} aria-label="Route">
      <div className="ap-route-head"><b>Why {decision.engine} was picked</b><Chip tone="plain">Preview</Chip></div>
      <div className="ap-route-cols">
        <div className="ap-route-col">
          <section className="ap-box">
            <h4>Chosen: {decision.engine} · {decision.model} · {effort}</h4>
            {decision.reasons.map((reason) => <p key={reason.code}>{reason.text}</p>)}
          </section>
          <Task row={row} />
        </div>
        <Considered rejected={decision.rejected} row={row} chosen={`${decision.engine} · ${decision.model}`} />
      </div>
      <p className="ap-muted">Rules run before any engine is scored, and a rule blocks an engine in every mode. A person can pin an engine the rule allows; to allow another, change the rule in {FILE}. Every routing decision, and the rule that made it, is in the ledger.</p>
      <p className="ap-muted">{GUARD_WORKERS}</p>
    </section>
  );
}

/** The Route card of one task. Nothing is drawn while no rule took part in its routing. */
export function RouteCard({ task }: { task: string }) {
  const read = useAsync(`crew-route:${task}`, (signal) => crewApi.route(task, signal));
  if (read.status === "loading") return <Loading label="Reading the route" rows={3} />;
  if (read.status === "error") return <ErrorState title="Can't read the route" failure={read.error} onRetry={read.reload} />;
  return <RouteClassCard view={read.data} onCheck={read.reload} />;
}
