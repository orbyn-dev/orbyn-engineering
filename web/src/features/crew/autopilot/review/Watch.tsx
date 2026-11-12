import { Banner } from "./Banner";
import { CrewFrame } from "./CrewFrame";
import { Act, Box, Mark, Status, type StatusTone } from "./Parts";
import type { CommitState, WatchData } from "./model";

const COMMIT: Record<CommitState, { tone: StatusTone; label: string }> = {
  untested: { tone: "neutral", label: "Not tested" },
  passes: { tone: "ok", label: "Passes here" },
  testing: { tone: "info", label: "Testing now" },
  fails: { tone: "err", label: "Fails here" },
  "not-needed": { tone: "neutral", label: "Not needed" },
};

/** Main is watched after every merge. A failed check on main is red; Crew finds the merge, and asks when it is not certain. */
export function WatchOnMain({ data }: { data: WatchData }) {
  return (
    <>
      <div className={`crew-main ${data.strip.tone === "bad" ? "crew-main-bad" : ""}`} role={data.strip.tone === "bad" ? "alert" : "status"}>
        <span>
          {data.strip.heading ? <b>{data.strip.heading}</b> : null}
          {data.strip.heading && data.strip.text ? " " : null}
          {data.strip.text}
        </span>
        {data.strip.actions.length ? (
          <span className="crew-bn-acts">
            {data.strip.actions.map((action) => (
              <Act key={action.label} label={action.label} primary={action.primary} />
            ))}
          </span>
        ) : null}
      </div>
      <div className="crew-pad crew-stack">
        {data.ask ? <Banner tone="amber" heading={data.ask.heading} text={data.ask.text} actions={data.ask.actions} /> : null}
        {data.list ? (
          <section className="crew-clist" aria-label={data.list.heading}>
            <div className="crew-lh">
              <b>{data.list.heading}</b>
              <span>{data.list.summary}</span>
            </div>
            <ol>
              {data.list.rows.map((row) => {
                const state = COMMIT[row.state];
                return (
                  <li key={row.sha} className={`crew-mq crew-mq-sha${row.state === "testing" ? " crew-mq-run" : ""}${row.state === "fails" ? " crew-mq-out" : ""}`}>
                    <span className="crew-sha">{row.sha}</span>
                    <div>
                      <b>{row.title}</b>
                      <small>{row.merged}</small>
                    </div>
                    <Status tone={state.tone} working={row.state === "testing"}>
                      {state.label}
                    </Status>
                  </li>
                );
              })}
            </ol>
          </section>
        ) : null}
        {data.evidence ? (
          <section className="crew-clist" aria-label={data.evidence.heading}>
            <div className="crew-lh">
              <b>{data.evidence.heading}</b>
              <span>{data.evidence.summary}</span>
            </div>
            <ul className="crew-claims">
              {data.evidence.rows.map((row) => (
                <li key={row.evidence} className={row.ok ? undefined : "crew-claim-bad"}>
                  <Mark ok={row.ok} />
                  <div>
                    {row.text ? <b>{row.text}</b> : null}
                    <span className="crew-evidence">{row.evidence}</span>
                    {row.note ? <span className="crew-cmn">{row.note}</span> : null}
                  </div>
                  <Status tone={row.ok ? "ok" : "err"}>{row.ok ? "Passed" : "Failed"}</Status>
                </li>
              ))}
            </ul>
          </section>
        ) : null}
        {data.after ? (
          <Box heading={data.after.heading}>
            <p>{data.after.text}</p>
          </Box>
        ) : null}
      </div>
    </>
  );
}

export function WatchScreen({ data }: { data: WatchData }) {
  return (
    <CrewFrame tab="Review" tool="review" reviewCount={7} mergeCount={3} health={data.health}>
      <WatchOnMain data={data} />
    </CrewFrame>
  );
}
