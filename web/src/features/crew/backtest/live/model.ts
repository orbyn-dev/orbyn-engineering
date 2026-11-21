import type { BacktestLaneRun } from "../../../../generated/BacktestLaneRun";
import type { BacktestRun } from "../../../../generated/BacktestRun";
import type { BacktestState } from "../../../../generated/BacktestState";
import type { LaneScore } from "../../../../generated/LaneScore";

/**
 * Why each Planned pill on the backtest screens is not built, in the words its pill gives. The first two name exactly what
 * a start request needs and no route serves: the lane list (each lane's review-or-check kind) and a token budget per
 * engine. A start is also refused by the service until a live replay engine is connected (A5).
 */
export const WHY = {
  settings: "the Settings › Reviewer backtest page is drawn only; no route serves it, so there is nothing to open",
  lanes: "the app has no read of the reviewer-backtest lanes, their engines or their models",
  budgets: "the app has no read of the per-engine token budgets: the Settings rows are drawn and the values are Proposed",
  tokens:
    "the per-engine token estimate is not in the estimate route yet (4n PR 3 / A5), so no estimate per lane is shown",
  start:
    "a start needs the lane list (lane, engine, model, review or check) and a token budget per engine, and no route serves the lanes or budgets before a run exists, so the app cannot send a start without inventing them; the service also refuses a start until a live replay engine is connected (A5)",
  runAgain:
    "the report serves each lane's name, engine and model and the run's per-engine budgets, but not whether that lane replayed a review or a check, so the app cannot rebuild the start request without guessing that field",
  replays:
    "the report route does not serve how many pull requests each lane replayed",
} as const;

/** The run a report frame names in its hash ('?id=<run id>'); empty when the frame is opened without one. */
export const backtestIdOfHash = (): string => new URLSearchParams(window.location.hash.split("?")[1] ?? "").get("id") ?? "";

export const count = (n: number) => n.toLocaleString("en-US");

const trim = (text: string) => text.replace(/\.0+$/, "").replace(/(\.\d)0$/, "$1");

/** Tokens in the engine's own unit, as the designs write them: 1.28M, 396k, none. Never a money figure. */
export function tokens(n: number): string {
  if (n <= 0) return "No tokens";
  if (n >= 1_000_000) return `${trim((n / 1_000_000).toFixed(2))}M tokens`;
  if (n >= 1_000) return `${trim((n / 1_000).toFixed(1))}k tokens`;
  return `${count(n)} tokens`;
}

/** A permille rate rounded down, as the design writes it: 666 → 66%. */
export const percent = (permille: number) => `${Math.floor(permille / 10)}%`;

/** "6 of 9 · recall 66%" once the sample is big enough, and nothing when the rate is not shown. */
export const rateText = (rate: { numerator: number; denominator: number; permille: number } | null, word: "recall" | "precision"): string | null =>
  rate ? `${count(rate.numerator)} of ${count(rate.denominator)} · ${word} ${percent(rate.permille)}` : null;

/** The lane as the read names it: no display name for an engine or model is served, so its own id is shown. */
export const laneTitle = (lane: BacktestLaneRun) => `${lane.lane} · ${lane.engine} · ${lane.model}`;

export const laneStateText: Record<BacktestLaneRun["state"], string> = {
  pending: "Not started",
  running: "Running",
  stopped: "Stopped",
  failed: "Failed",
  done: "Done",
};

/** What a lane's outcome cell says, and the sample note under it. A Not measured lane never shows a zero rate. */
export function escapesText(lane: BacktestLaneRun): { value: string; note: string } {
  const recall = rateText(lane.score?.recall ?? null, "recall");
  if (recall) return { value: `${count(lane.score?.recall?.numerator ?? 0)} of ${count(lane.score?.recall?.denominator ?? 0)}`, note: `recall ${percent(lane.score?.recall?.permille ?? 0)}` };
  if (lane.outcome === "not_measured") return { value: "Not measured", note: "the escape log can’t say" };
  if (lane.outcome === "unscored") return { value: "Not scored", note: lane.state === "failed" ? "the lane failed" : lane.state === "stopped" ? "the lane stopped" : "it has not finished" };
  return { value: "Too few for a rate", note: "counts only" };
}

/** What the findings cell says: the count raised, and the unresolved escapes counted beside the score. */
export function findingsText(lane: BacktestLaneRun): { value: string; note: string } {
  const value = count(lane.findings_n);
  const note = lane.unresolved_n === null ? "" : `${count(lane.unresolved_n)} unresolved`;
  return { value, note };
}

/** What the matched cell says: precision when it shows, else the unmatched count, never a rate below its minimum. */
export function matchedText(lane: BacktestLaneRun): { value: string; note: string } {
  const precision = rateText(lane.score?.precision ?? null, "precision");
  if (precision) return { value: `${count(lane.score?.precision?.numerator ?? 0)} of ${count(lane.score?.precision?.denominator ?? 0)}`, note: `precision ${percent(lane.score?.precision?.permille ?? 0)}` };
  if (lane.outcome === "not_measured") return { value: "Not measured", note: "the escape log can’t say" };
  if (lane.outcome === "unscored") return { value: "Not scored", note: "no findings were scored" };
  return { value: "Too few for a rate", note: lane.score ? `${count(lane.score.unmatched_n)} unmatched` : "counts only" };
}

/** How the run's outcomes stand, from its lanes: never "measured" when any lane could not be scored. */
export function outcomesSummary(lanes: readonly BacktestLaneRun[]): { value: string; note: string } {
  if (lanes.length === 0) return { value: "No lane", note: "the run recorded no lane" };
  const scored = lanes.filter((lane) => lane.outcome === "scored").length;
  if (scored === lanes.length) return { value: "Measured", note: "every lane was scored against the escape log" };
  if (scored === 0) return { value: "Not measured", note: "the escape log can’t say, so finding counts show and no rate is a zero" };
  return { value: "Partly measured", note: "the lanes the log could not score show finding counts only" };
}

/** Each engine's own tokens used, in its own unit and never added together, in the order the lanes name the engines. */
export function usedByEngine(lanes: readonly BacktestLaneRun[]): { engine: string; tokens: number }[] {
  const used = new Map<string, number>();
  for (const lane of lanes) used.set(lane.engine, (used.get(lane.engine) ?? 0) + lane.tokens_used);
  return [...used].map(([engine, tokens]) => ({ engine, tokens }));
}

export const scoredLanes = (lanes: readonly BacktestLaneRun[]) => lanes.filter((lane) => lane.outcome === "scored").length;

/** The chip a run's state gets in the header: a stopped run is the Partial the design names. */
export const runStateText: Record<BacktestState, string> = {
  estimated: "Estimated",
  running: "Running",
  stopped: "Partial",
  finished: "Finished",
  failed: "Failed",
};

/** Sat 7 Nov: the day a run was created, as the design dates it. */
export function runDay(unix: number): string {
  return new Date(unix * 1000).toLocaleDateString("en-GB", { weekday: "short", day: "numeric", month: "short" });
}

/** A stop reason in the service's own words, made readable without changing what it says. */
export function stopReasonText(reason: string | null): string {
  if (!reason) return "No reason was recorded.";
  if (reason === "stopped by the person") return "You stopped it.";
  if (reason === "interrupted") return "The service stopped while it was running (interrupted).";
  if (reason.startsWith("token budget spent:")) return `The token budget was spent for ${reason.slice("token budget spent:".length).trim()}.`;
  return reason;
}

/** The lane's own score, if the read has one. */
export const laneScore = (lane: BacktestLaneRun): LaneScore | null => lane.score;

/** The run's own date, and its finish day only when the run ended on another day: one day is one date, not two. */
export function runDate(run: BacktestRun): string {
  const day = runDay(run.created_unix);
  if (run.finished_unix === null) return day;
  const sameDay = new Date(run.created_unix * 1000).toDateString() === new Date(run.finished_unix * 1000).toDateString();
  return sameDay ? day : `${day} · finished ${runDay(run.finished_unix)}`;
}
