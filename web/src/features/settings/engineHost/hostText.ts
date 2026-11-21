import type { HostRow, HostState } from "./contract";

export interface Chip {
  label: string;
  /** Green only for Running, blue for a wait that ends by itself, red for what failed, plain for the rest. */
  tone: "ok" | "info" | "err" | "plain";
  spin?: boolean;
}

export const CHIPS: Record<HostState, Chip> = {
  not_started: { label: "Not started", tone: "plain" },
  starting: { label: "Starting", tone: "info", spin: true },
  running: { label: "Running", tone: "ok" },
  stopped: { label: "Stopped", tone: "plain" },
  crashed: { label: "Crashed", tone: "err" },
  restarting: { label: "Restarting", tone: "info", spin: true },
  cant_start: { label: "Can’t start", tone: "err" },
  version_mismatch: { label: "Version mismatch", tone: "err" },
  not_used: { label: "Not used", tone: "plain" },
};

/** The clock as the Maintenance list writes it: 24-hour, no seconds. */
export function clock(iso: string | null | undefined): string {
  const at = iso ? new Date(iso) : null;
  return at && !Number.isNaN(at.getTime()) ? at.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false }) : "";
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** `21 Nov`, as the frames write a date, in every locale. */
export function shortDate(iso: string | null | undefined): string {
  const at = iso ? new Date(iso) : null;
  return at && !Number.isNaN(at.getTime()) ? `${at.getDate()} ${MONTHS[at.getMonth()]}` : "";
}

function ago(iso: string | null | undefined, now: number): string {
  const minutes = iso ? Math.max(0, Math.round((now - Date.parse(iso)) / 60_000)) : 0;
  if (!iso || Number.isNaN(minutes) || minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} ${minutes === 1 ? "minute" : "minutes"} ago`;
  const hours = Math.round(minutes / 60);
  return `${hours} ${hours === 1 ? "hour" : "hours"} ago`;
}

const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;
const sentence = (text: string) => (/[.!?]$/.test(text) ? text : `${text}.`);

/** `Restart 3 of 5 · next try in 4 s`: the try, and when the next one is. */
export function restartLine(row: HostRow): string {
  const restart = row.restart;
  if (!restart) return "Waiting to start it again.";
  const next = restart.next_try_seconds;
  return `Restart ${restart.attempt} of ${restart.limit}${next === null ? "" : ` · next try in ${next} s`}`;
}

/** The mismatch sentence the protocol gives, with both numbers. */
export const mismatchLine = (row: HostRow) =>
  `The engine host does not match this app version.${row.protocol != null && row.app_protocol != null ? ` Host protocol ${row.protocol}, app protocol ${row.app_protocol}.` : ""}`;

/** The one line a row in the Providers group says under its name. */
export function listLine(row: HostRow, now = Date.now()): string {
  switch (row.state) {
    case "not_started": return "Starts with the first thread that uses it.";
    case "starting": return "Waiting for the host to say hello";
    case "running": return row.pooled
      ? `One host for every ${row.name} thread · since ${clock(row.since)}`
      : `Host ${row.host_version ?? ""} · protocol ${row.protocol ?? ""} · since ${clock(row.since)}`;
    case "stopped": return `Stopped ${ago(row.stopped_at, now)} because nothing was using it. Starts with the next thread.`;
    case "crashed": return `Stopped at ${clock(row.stopped_at)}.${crashedThreads(row)} Restarting next.`;
    case "restarting": return restartLine(row);
    case "cant_start": return `Stopped ${row.stops_in_window ?? 5} times in 5 minutes.${row.last_error ? ` Last error: ${sentence(row.last_error.message)}` : ""}`;
    case "version_mismatch": return mismatchLine(row);
    case "not_used": return `${row.name} runs through its own program here.`;
  }
}

function crashedThreads(row: HostRow): string {
  return row.failed_threads ? ` ${plural(row.failed_threads, "thread", "threads")} failed.` : "";
}

/** The State row of the card on a provider's detail page. */
export function stateLine(row: HostRow, today = new Date()): string {
  switch (row.state) {
    case "starting": return "Waiting for the host to say hello. 10 seconds; 30 the first time after an update.";
    case "running": {
      const since = row.since ? new Date(row.since) : null;
      const same = since !== null && since.toDateString() === today.toDateString();
      return `Since ${clock(row.since)}${same ? " today" : since ? ` on ${shortDate(row.since)}` : ""}`;
    }
    case "stopped": return "Stopped because nothing was using it. Starts with the next thread.";
    case "version_mismatch": return "Refused at hello. Nothing is running on it.";
    default: return listLine(row, today.getTime());
  }
}

const CAPABILITY_WORDS: Record<string, string> = {
  steer: "Steer a run", fork: "fork", background: "background work", rollback: "roll back", models: "list models", usage: "report usage",
};

/** What the host said it can do, in its own order. A word this app does not know is shown as it came. */
export const capabilityLine = (capabilities: string[]) =>
  capabilities.map((name, at) => {
    const word = CAPABILITY_WORDS[name] ?? name;
    return at === 0 ? word.charAt(0).toUpperCase() + word.slice(1) : word;
  }).join(", ");

/** The banner above a detail page when the host crashed or was refused. Empty when there is nothing to say. */
export function bannerLine(row: HostRow): string | null {
  if (row.state === "version_mismatch") {
    return `The engine host does not match this app version. The host speaks protocol ${row.protocol ?? "?"}; this app speaks protocol ${row.app_protocol ?? "?"}.`;
  }
  if (row.state === "crashed" || row.state === "restarting") {
    const threads = row.failed_threads ? ` ${plural(row.failed_threads, "thread", "threads")} that ${row.failed_threads === 1 ? "was" : "were"} running on it failed.` : "";
    return `The ${row.name} engine host stopped${row.stopped_at ? ` at ${clock(row.stopped_at)}` : ""}.${threads} Your work in the project is kept.`;
  }
  return null;
}
