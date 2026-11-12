import { askedBy, hardCell, leadText, ResultChip, secondText } from "./parts";
import type { ApprovalRow } from "./model";

/** The wide log: one row per request with its whole decision chain. Rows are buttons in a list. */
export function ApprovalTable({ rows, selected, onSelect }: { rows: ApprovalRow[]; selected: string | null; onSelect(id: string): void }) {
  return (
    <div className="ap-log" role="table" aria-label="Approval log">
      <div className="ap-row ap-head" role="row">
        {["Time", "Request", "Hard rule (code)", "The lead", "Second key", "Result"].map((name) => (
          <span key={name} role="columnheader">{name}</span>
        ))}
      </div>
      {rows.map((row) => (
        <div key={row.id} role="row" className={`ap-row${selected === row.id ? " ap-sel" : ""}`}>
          <span role="cell" className="ap-num">{row.time}</span>
          <span role="cell">
            <button type="button" className="ap-open" aria-pressed={selected === row.id} onClick={() => onSelect(row.id)}>
              <b>{row.title}</b>
              <small>{askedBy(row)}</small>
            </button>
          </span>
          <span role="cell">{hardCell(row)}</span>
          <span role="cell" className="ap-clip">{leadText(row)}</span>
          <span role="cell" className="ap-clip">{secondText(row)}</span>
          <span role="cell"><ResultChip result={row.result} /></span>
        </div>
      ))}
    </div>
  );
}

/** The 320 px side panel: each request is a small card. Selecting one opens its chain in Crew. */
export function ApprovalCards({ rows, onSelect }: { rows: ApprovalRow[]; onSelect(id: string): void }) {
  return (
    <ul className="ap-cards" aria-label="Approval log">
      {rows.map((row) => (
        <li key={row.id}>
          <button type="button" className="ap-card" onClick={() => onSelect(row.id)}>
            <span className="ap-card-top">
              <span className="ap-num">{row.time}</span>
              <ResultChip result={row.result} />
            </span>
            <b>{row.title}</b>
            <small>{askedBy(row)}</small>
          </button>
        </li>
      ))}
    </ul>
  );
}
