import type { Diagnostics } from "../../../api/types";
import type { HostCrash } from "./contract";
import { CauseChain, DetailsToggle } from "./HostParts";
import "./engineHost.css";

/** A Recent problems row: an ordinary service event, or a host crash that can open its details. */
export type ProblemRow = Diagnostics["events"][number] | HostCrash;

const threads = (count: number) => `${count} ${count === 1 ? "thread" : "threads"}`;

/** After the kind: which try the restart is on, and how many threads failed. */
const crashExtra = (crash: HostCrash) =>
  [crash.restart ? `restart ${crash.restart.attempt} of ${crash.restart.limit}` : null, `${threads(crash.failed_threads)} failed`].filter(Boolean).join(" · ");

/** The open row as text, for Copy report. The host redacts prompts and tokens before it sends anything. */
export function crashReport(crash: HostCrash): string[] {
  return [`  ${[crash.kind ? `Kind: ${crash.kind}` : null, crashExtra(crash)].filter(Boolean).join(" · ")}`, ...crash.causes.map((cause, at) => `  ${at + 1} ${cause.name}: ${cause.message}${cause.code ? ` (${cause.code})` : ""}`),
    ...(crash.stack ? ["  Stack", ...crash.stack.split("\n").map((line) => `    ${line}`)] : [])];
}

/** Recent events and host crashes in one list by time, newest first. With no crash it is the events exactly as the service sent them. */
export function mergeProblems(events: Diagnostics["events"], crashes: HostCrash[], windowMs: number, now = Date.now()): ProblemRow[] {
  const recent = crashes.filter((crash) => now - Date.parse(crash.at) <= windowMs);
  return recent.length === 0 ? events : [...events, ...recent].sort((a, b) => Date.parse(b.at) - Date.parse(a.at));
}

/** One problem as report lines; an open crash adds its chain and stack. */
export function problemReport(row: ProblemRow, time: string, openCrash: string | null): string[] {
  return "failed_threads" in row ? [`${time} ${row.message}`, ...(openCrash === row.id ? crashReport(row) : [])] : [`${time} ${row.message}`];
}

/** One host crash under Recent problems. Show opens the cause chain and the stack, which is shown nowhere else. */
export function CrashProblem({ crash, time, open, onToggle }: { crash: HostCrash; time: string; open: boolean; onToggle(): void }) {
  return (
    <div role="row" className="set-tr eh-problem">
      <span role="cell" className="set-time">{time}</span>
      <span role="cell">{crash.message}</span>
      <span role="cell"><DetailsToggle open={open} onToggle={onToggle} /></span>
      {open ? (
        <div role="cell" className="eh-problem-detail">
          <CauseChain extra={crashExtra(crash)} kind={crash.kind} causes={crash.causes} />
          {crash.stack ? <><div className="eh-stack-label">Stack</div><pre className="eh-stack">{crash.stack}</pre></> : null}
        </div>
      ) : null}
    </div>
  );
}

/** One Recent problems row of either kind: an ordinary event keeps its two columns, a host crash can open. */
export function ProblemLine({ row, time, open, onToggle }: { row: ProblemRow; time: string; open: boolean; onToggle(): void }) {
  return "failed_threads" in row
    ? <CrashProblem crash={row} time={time} open={open} onToggle={onToggle} />
    : <div role="row" className="set-tr" style={{ gridTemplateColumns: "90px minmax(0, 1fr)" }}>
      <span role="cell" className="set-time">{time}</span><span role="cell">{row.message}</span>
    </div>;
}
