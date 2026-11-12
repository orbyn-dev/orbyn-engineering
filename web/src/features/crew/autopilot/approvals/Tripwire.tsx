import { useRef } from "react";
import { useAsync } from "../../../../lib/useAsync";
import { crewApi } from "./api";
import { rowOf, stopOf } from "./ledger";
import { useAutopilotGuards } from "./useCrew";
import { Button } from "../../../../ui/controls";
import { Chip } from "./parts";
import type { TripwireStop } from "./model";
import { GUARD_WORKERS } from "./copy";

type Limits = { inARow: number; inARun: number };

const whyDenials = (limits?: Limits) =>
  limits
    ? `The app stops the task on ${limits.inARow} denials in a row, with the reason “too many denials”. The second rule is ${limits.inARun} denials in one run. Text that looks like injected instructions stops it too. Stopping is not a failure of the task’s checks, so it isn’t red.`
    : "The app stops the task on a run of denials in a row, with the reason “too many denials”. The second rule is a limit on denials in one run. Text that looks like injected instructions stops it too. Stopping is not a failure of the task’s checks, so it isn’t red.";
const whyText = (limits?: Limits) =>
  `Text a worker reads is data, never an instruction. When text in a page or file looks like it is telling the agent what to do, Crew stops the task and shows you the text instead of acting on it. ${limits ? `${limits.inARow} denials in a row or ${limits.inARun} in a run` : "Denials in a row or in one run"} stop a task too, with the reason “too many denials”. Stopping is not a failure of the task’s checks, so it isn’t red.`;

/** A task stopped by a trip wire. Amber: it needs a new brief from you, and the task's checks did not fail. */
export function Tripwire({ stop, limits, onNewBrief }: { stop: TripwireStop; limits?: Limits; onNewBrief?(stop: TripwireStop): void }) {
  const table = useRef<HTMLDivElement>(null);
  const injected = stop.reason === "injected_instructions";
  const quoted = injected ? stop.injected : null;
  const who = stop.engine ? `${stop.engine.engine}’s task “${stop.taskTitle}”` : `The task “${stop.taskTitle}”`;
  return (
    <section className="ap-stop" aria-label={`Stopped: ${stop.taskTitle}`}>
      <div className="ap-banner ap-banner-am" role="status">
        <p>
          <b>{injected ? "Stopped · possible injected instructions." : "Stopped · too many denials."}</b>{" "}
          {injected
            ? `${who} read text that looks like instructions to the agent, so Crew stopped it. Nothing was lost; its worktree is intact.`
            : `${who} was denied ${stop.denialCount} times ${stop.inARun ? "in one run" : "in a row"}, so Crew stopped it. Nothing was lost; its worktree is intact.`}
        </p>
        <span className="ap-banner-acts">
          {quoted || !injected ? <Button small onClick={() => table.current?.focus()}>{quoted ? "Review the text" : "Review the denials"}</Button> : null}
          {onNewBrief ? <Button small tone="primary" onClick={() => onNewBrief(stop)}>Write a new brief</Button> : null}
        </span>
      </div>
      {injected && !quoted ? null : <div ref={table} tabIndex={-1} className="ap-stop-table" role="table" aria-label={injected ? "The text" : "Denied requests"}>
        {quoted ? (
          <>
            <div className="ap-stop-row ap-head ap-stop-inj" role="row">
              {["Where", "The text, shown as plain data", "Result"].map((name) => <span key={name} role="columnheader">{name}</span>)}
            </div>
            <div className="ap-stop-row ap-stop-inj" role="row">
              <span role="cell"><b>{quoted.where}</b><small>{quoted.source}</small></span>
              <span role="cell"><code className="ap-data">{quoted.text}</code></span>
              <span role="cell"><Chip tone="am">Stopped</Chip></span>
            </div>
          </>
        ) : (
          <>
            <div className="ap-stop-row ap-head" role="row">
              {["Time", "Request", "Why it was denied", "Result"].map((name) => <span key={name} role="columnheader">{name}</span>)}
            </div>
            {stop.denials.map((denial) => (
              <div key={`${denial.time}-${denial.title}`} className="ap-stop-row" role="row">
                <span role="cell" className="ap-num">{denial.time}</span>
                <span role="cell"><b>{denial.title}</b></span>
                <span role="cell" className="ap-clip">{denial.why}</span>
                <span role="cell"><Chip tone="no">{denial.result}</Chip></span>
              </div>
            ))}
          </>
        )}
      </div>}
      <section className="ap-box">
        <h4>Why Crew stops</h4>
        <p>{injected ? whyText(limits) : whyDenials(limits)}</p>
        {injected ? <p className="ap-muted">{GUARD_WORKERS}</p> : null}
      </section>
    </section>
  );
}

/** The tasks Crew stopped lately, each with the requests that were denied. */
export function StopsList() {
  const stops = useAsync("crew-stops", (signal) => crewApi.stops(signal), { keep: true });
  if (stops.status !== "ready") return null;
  const limits = { inARow: stops.data.in_a_row_limit, inARun: stops.data.in_a_run_limit };
  return (
    <>
      {stops.data.recent.filter((row) => row.kind === "tripwire").map((row) => <StopFor key={row.id} limits={limits} stop={stopOf(row, limits)} />)}
    </>
  );
}

function StopFor({ stop, limits }: { stop: TripwireStop; limits: Limits }) {
  const guards = useAutopilotGuards();
  const rules = guards.data?.always_denied ?? [];
  const denied = useAsync(`crew-stop-${stop.taskId}`, (signal) => crewApi.approvals({ task: stop.taskId, kind: "denied", offset: 0, limit: 50 }, signal));
  const denials = (denied.data?.rows ?? []).flatMap((row) => {
    const one = rowOf(row, rules);
    if (!one) return [];
    return [{ time: one.time, title: one.title, why: one.lead.state === "no" ? one.lead.reason : one.rule ?? "", result: one.result === "denied_rule" ? ("Denied by rule" as const) : ("Denied by lead" as const) }];
  });
  return <Tripwire stop={{ ...stop, denials: denials.slice(0, stop.denialCount || denials.length).reverse() }} limits={limits} />;
}
