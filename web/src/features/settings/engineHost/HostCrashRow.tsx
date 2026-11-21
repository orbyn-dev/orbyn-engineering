import type { Diagnostics } from "../../../api/types";
import { type HostCrash, errorKindLine, exitLine, plain } from "../../enginehost";
import { CrashDetails, DetailsToggle, hasCrashDetails } from "./HostParts";
import "./engineHost.css";

/** A Recent problems row: an ordinary service event, or a host crash that can open its details. */
export type ProblemRow = Diagnostics["events"][number] | HostCrash;

/** How the host process ended and how it failed, as one line. */
const crashSummary = (crash: HostCrash) => [exitLine(crash), errorKindLine(crash.error_kind)].filter(Boolean).join(" · ");

/** The open row as text, for Copy report. The core redacts the stderr tail and caps it before it sends anything. */
export function crashReport(crash: HostCrash): string[] {
  const tail = typeof crash.stderr_tail === "string" ? plain(crash.stderr_tail, 1024) : "";
  return [
    ...(crashSummary(crash) ? [`  ${crashSummary(crash)}`] : []),
    ...(tail ? ["  Last words from the host", ...tail.split("\n").map((line) => `    ${line}`)] : []),
  ];
}

/** Recent events and host crashes in one list by time, newest first. With no crash it is the events exactly as the service sent them. */
export function mergeProblems(events: Diagnostics["events"], crashes: HostCrash[], windowMs: number, now = Date.now()): ProblemRow[] {
  const recent = crashes.filter((crash) => now - Date.parse(crash.at) <= windowMs);
  return recent.length === 0 ? events : [...events, ...recent].sort((a, b) => Date.parse(b.at) - Date.parse(a.at));
}

/** One problem as report lines; an open crash adds its chain and stack. */
export function problemReport(row: ProblemRow, time: string, openCrash: string | null): string[] {
  return "stderr_tail" in row ? [`${time} ${row.message}`, ...(openCrash === row.id ? crashReport(row) : [])] : [`${time} ${row.message}`];
}

/** One host crash under Recent problems. Show opens the cause chain and the stack, which is shown nowhere else. */
export function CrashProblem({ crash, time, open, onToggle }: { crash: HostCrash; time: string; open: boolean; onToggle(): void }) {
  return (
    <div role="row" className="set-tr eh-problem">
      <span role="cell" className="set-time">{time}</span>
      <span role="cell">{plain(crash.message, 200)}</span>
      <span role="cell">{hasCrashDetails(crash) ? <DetailsToggle open={open} onToggle={onToggle} /> : null}</span>
      {open && hasCrashDetails(crash) ? (
        <div role="cell" className="eh-problem-detail">
          <CrashDetails crash={crash} />
        </div>
      ) : null}
    </div>
  );
}

/** One Recent problems row of either kind: an ordinary event keeps its two columns, a host crash can open. */
export function ProblemLine({ row, time, open, onToggle }: { row: ProblemRow; time: string; open: boolean; onToggle(): void }) {
  return "stderr_tail" in row
    ? <CrashProblem crash={row} time={time} open={open} onToggle={onToggle} />
    : <div role="row" className="set-tr" style={{ gridTemplateColumns: "90px minmax(0, 1fr)" }}>
      <span role="cell" className="set-time">{time}</span><span role="cell">{row.message}</span>
    </div>;
}
