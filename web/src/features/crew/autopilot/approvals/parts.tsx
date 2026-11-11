import type { ReactNode } from "react";
import { HARD_CELL, RESULT } from "./copy";
import type { ApprovalRow, EngineRef, ResultKind, Risk } from "./model";

export type ChipTone = "ok" | "no" | "am" | "dim" | "plain";

export function Chip({ tone = "plain", children }: { tone?: ChipTone; children: ReactNode }) {
  return <span className={`ap-chip ap-chip-${tone}`}>{children}</span>;
}

export const ResultChip = ({ result }: { result: ResultKind }) => <Chip tone={RESULT[result].tone}>{RESULT[result].label}</Chip>;

export const RISK_LABEL: Record<Risk, string> = { low: "Low risk", medium: "Medium risk", high: "High risk" };

export const engineLine = (engine: EngineRef) => `${engine.engine} · ${engine.model}`;

/** "Claude Code · Sonnet 5.5 · inside its lease": who asked and the row's own notes. */
export function askedBy(row: ApprovalRow) {
  const parts = [engineLine(row.asker)];
  if (row.risk) parts.push(RISK_LABEL[row.risk].toLowerCase());
  return [...parts, ...row.notes].join(" · ");
}

export function hardCell(row: ApprovalRow) {
  const cell = HARD_CELL[row.hard];
  return <Chip tone={cell.tone}>{cell.label}</Chip>;
}

export function leadText(row: ApprovalRow): string {
  switch (row.lead.state) {
    case "not_asked":
      return "Not asked";
    case "yes":
      return `Yes: ${row.lead.short}`;
    case "no":
      return `No: ${row.lead.short}`;
  }
}

export function secondText(row: ApprovalRow): string {
  const second = row.second;
  switch (second.state) {
    case "not_needed":
      return "Not needed";
    case "not_asked":
      return "Not asked";
    case "unavailable":
      return "No second vendor is signed in";
    case "you":
      return "You · once";
    case "yours":
      return "The decision is yours";
    case "reviewing":
      return `${second.by.engine} · reviewing`;
    default:
      return `${second.by.engine} · ${second.state}`;
  }
}

export function Facts({ rows }: { rows: { label: string; text: string }[] }) {
  return (
    <dl className="ap-facts">
      {rows.map((row) => (
        <div key={row.label}>
          <dt>{row.label}</dt>
          <dd>{row.text}</dd>
        </div>
      ))}
    </dl>
  );
}
