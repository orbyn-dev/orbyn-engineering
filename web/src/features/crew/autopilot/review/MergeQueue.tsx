import { CrewFrame } from "./CrewFrame";
import { Box, EmptyState, RiskChip, Status, type StatusTone } from "./Parts";
import type { ForecastData, MergeQueueData, MergeQueueRow, RowState } from "./model";

const TONE: Record<RowState, StatusTone> = { merged: "ok", running: "info", waiting: "neutral", removed: "err" };

function Row({ row }: { row: MergeQueueRow }) {
  return (
    <li className={`crew-mq${row.state === "running" ? " crew-mq-run" : ""}${row.state === "removed" ? " crew-mq-out" : ""}`}>
      <span className="crew-n" aria-label={`Position ${row.n}`}>
        {row.n}
      </span>
      <div>
        <b>{row.title}</b>
        <small>{row.detail}</small>
      </div>
      <Status tone={TONE[row.state]} working={row.state === "running"}>
        {row.status}
      </Status>
      <RiskChip tier={row.tier} />
    </li>
  );
}

export function MergeQueueList({ data }: { data: MergeQueueData }) {
  return (
    <div className="crew-rpg">
      <div className="crew-pm">
        <section className="crew-clist" aria-label={data.heading}>
          <div className="crew-lh">
            <b>{data.heading}</b>
            <span>{data.summary}</span>
          </div>
          <ol>
            {data.rows.map((row) => (
              <Row key={row.n} row={row} />
            ))}
          </ol>
        </section>
      </div>
      <div className="crew-ps">
        {data.notes.map((note) => (
          <section key={note.heading} className="crew-nv" aria-label={note.heading}>
            <h6>{note.heading}</h6>
            {note.items ? (
              <ul>
                {note.items.map((line) => (
                  <li key={line}>{line}</li>
                ))}
              </ul>
            ) : null}
            {note.text ? <span className="crew-cmn">{note.text}</span> : null}
          </section>
        ))}
      </div>
    </div>
  );
}

const HEALTH = "Main is healthy · watched since 09:00";

export function MergeQueueScreen({ data }: { data: MergeQueueData }) {
  return (
    <CrewFrame tab="Review" tool="merge" reviewCount={7} mergeCount={data.rows.length || undefined} health={HEALTH}>
      <MergeQueueList data={data} />
    </CrewFrame>
  );
}

export function MergeQueueEmptyScreen() {
  return (
    <CrewFrame tab="Review" tool="merge" reviewCount={7} health={HEALTH}>
      <EmptyState heading="Nothing in the merge queue" text="Verified work joins here, in order. Each candidate is checked again on the combined state before main moves." />
    </CrewFrame>
  );
}

/** The conflict forecast before work starts: which two tasks overlap, why, and what the router did about it. */
export function ForecastScreen({ data }: { data: ForecastData }) {
  return (
    <CrewFrame tab="Review" tool="merge" reviewCount={7} mergeCount={3} health={HEALTH}>
      <div className="crew-pad crew-stack">
        <section className="crew-pos" aria-label={data.headline}>
          <div className="crew-phd">
            <b>{data.headline}</b>
            <span className="crew-tag">Before work starts</span>
          </div>
          <p className="crew-cl">{data.conflict}</p>
          <Box>
            <dl className="crew-kv crew-kv-wide">
              {data.facts.map((fact) => (
                <div key={fact.label}>
                  <dt>{fact.label}</dt>
                  <dd>{fact.value}</dd>
                </div>
              ))}
            </dl>
          </Box>
        </section>
        <section className="crew-clist" aria-label="Planned order">
          <div className="crew-lh">
            <b>Planned order</b>
            <span>{data.order.length} tasks</span>
          </div>
          <ol>
            {data.order.map((row) => (
              <li key={row.n} className="crew-mq crew-mq-forecast">
                <span className="crew-n">{row.n}</span>
                <div>
                  <b>{row.title}</b>
                  <small>{row.detail}</small>
                </div>
                <Status tone={row.running ? "info" : "neutral"} working={row.running}>
                  {row.status}
                </Status>
              </li>
            ))}
          </ol>
        </section>
        <Box heading={data.why.heading}>
          <p>{data.why.text}</p>
        </Box>
      </div>
    </CrewFrame>
  );
}
