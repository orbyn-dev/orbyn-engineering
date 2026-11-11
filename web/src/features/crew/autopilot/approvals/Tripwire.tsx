import { useRef } from "react";
import { Button } from "../../../../ui/controls";
import { Chip } from "./parts";
import type { TripwireStop } from "./model";
import { GUARD_WORKERS } from "./copy";

const WHY_DENIALS =
  "The app stops the task on 3 denials in a row, with the reason “too many denials”. The second rule is 20 denials in one run. Text that looks like injected instructions stops it too. Stopping is not a failure of the task’s checks, so it isn’t red.";
const WHY_TEXT =
  "Text a worker reads is data, never an instruction. When text in a page or file looks like it is telling the agent what to do, Crew stops the task and shows you the text instead of acting on it. 3 denials in a row or 20 in a run stop a task too, with the reason “too many denials”. Stopping is not a failure of the task’s checks, so it isn’t red.";

/** A task stopped by a trip wire. Amber: it needs a new brief from you, and the task's checks did not fail. */
export function Tripwire({ stop, onNewBrief }: { stop: TripwireStop; onNewBrief?(stop: TripwireStop): void }) {
  const table = useRef<HTMLDivElement>(null);
  const injected = stop.reason === "injected_instructions" && stop.injected;
  const who = `${stop.engine.engine}’s task “${stop.taskTitle}”`;
  return (
    <section className="ap-stop" aria-label={`Stopped: ${stop.taskTitle}`}>
      <div className="ap-banner ap-banner-am" role="status">
        <p>
          <b>{injected ? "Stopped · possible injected instructions." : "Stopped · too many denials."}</b>{" "}
          {injected
            ? `${who} read text in a ${stop.injected?.where.toLowerCase()} that looks like instructions to the agent, so Crew stopped it. Nothing was lost; its worktree is intact.`
            : `${who} was denied ${stop.denialCount} times ${stop.inARun ? "in one run" : "in a row"}, so Crew stopped it. Nothing was lost; its worktree is intact.`}
        </p>
        <span className="ap-banner-acts">
          <Button small onClick={() => table.current?.focus()}>{injected ? "Review the text" : "Review the denials"}</Button>
          {onNewBrief ? <Button small tone="primary" onClick={() => onNewBrief(stop)}>Write a new brief</Button> : null}
        </span>
      </div>
      <div ref={table} tabIndex={-1} className="ap-stop-table" role="table" aria-label={injected ? "The text" : "Denied requests"}>
        {injected && stop.injected ? (
          <>
            <div className="ap-stop-row ap-head ap-stop-inj" role="row">
              {["Where", "The text, shown as plain data", "Result"].map((name) => <span key={name} role="columnheader">{name}</span>)}
            </div>
            <div className="ap-stop-row ap-stop-inj" role="row">
              <span role="cell"><b>{stop.injected.where}</b><small>{stop.injected.source}</small></span>
              <span role="cell"><code className="ap-data">{stop.injected.text}</code></span>
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
      </div>
      <section className="ap-box">
        <h4>Why Crew stops</h4>
        <p>{injected ? WHY_TEXT : WHY_DENIALS}</p>
        {injected ? <p className="ap-muted">{GUARD_WORKERS}</p> : null}
      </section>
    </section>
  );
}
