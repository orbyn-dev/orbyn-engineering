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
  if (!restart) return row.restart_count ? `Waiting to start it again. Restarted ${plural(row.restart_count, "time", "times")} since Orbyn started.` : "Waiting to start it again.";
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
      ? `One host for every ${plain(row.name, 60)} thread${sinceClock(row)}`
      : [`Host ${plain(row.host_version ?? "", 20)}`.trimEnd(), row.protocol != null ? `protocol ${row.protocol}` : null, sinceClock(row).slice(3) || null].filter(Boolean).join(" · ");
    case "stopped": return `Stopped ${ago(row.stopped_at, now)} because nothing was using it. Starts with the next thread.`;
    case "crashed": return `Stopped at ${clock(row.stopped_at)}.${crashedThreads(row)} Restarting next.`;
    case "restarting": return restartLine(row);
    case "cant_start": return `${cantStartLead(row)}${row.last_error ? ` Last error: ${sentence(plain(row.last_error.message, 200))}` : ""}`;
    case "version_mismatch": return mismatchLine(row);
    case "not_used": return `${plain(row.name, 60)} runs through its own program here.`;
    default: return "";
  }
}

/** ` · since 09:12` when the service says when it started running, nothing when it does not. */
const sinceClock = (row: HostRow): string => (row.since && clock(row.since) ? ` · since ${clock(row.since)}` : "");

/** What stopped a family that can't start: the service's own sentence, or the protocol's rule (five stops in five minutes). */
export const cantStartLead = (row: HostRow): string =>
  row.detail ? sentence(plain(row.detail, 200)) : `Stopped ${row.stops_in_window ?? 5} times in 5 minutes.`;

/** The Reason row of a Not used card. */
export const notUsedReason = (row: HostRow): string | null =>
  row.not_used === "flag_off" ? "The SDK path is off." : row.not_used === "unsupported_platform" ? "This build has no engine host for this platform." : null;

/** What a crash says in its details: how the host process ended. The exit code or signal appears only there. */
export function exitLine(crash: { exit_code?: number; signal?: number }): string | null {
  if (typeof crash.exit_code === "number") return `Exited with code ${crash.exit_code}`;
  if (typeof crash.signal === "number") return `Stopped by signal ${crash.signal}`;
  return null;
}

const ERROR_KIND_WORDS = new Map<string, string>(Object.entries({
  exited: "The host exited", closed_output: "The host closed its output", stalled: "The host stopped answering", malformed: "The host sent a frame this app could not read",
  oversize: "The host sent a frame that was too large", hello_timeout: "The host did not say hello in time", start_failed: "The host did not start",
}));

/** How the host failed, as a short phrase; a kind this app does not know is not shown. */
export const errorKindLine = (kind: string): string | null => (typeof kind === "string" ? ERROR_KIND_WORDS.get(kind) : undefined) ?? null;

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
    case "not_used": return `Not used. ${plain(row.name, 60)} keeps running through its own program.`;
    case "version_mismatch": return "Refused at hello. Nothing is running on it.";
    default: return listLine(row, today.getTime());
  }
}

const CAPABILITY_WORDS = new Map<string, string>(Object.entries({
  steer: "Steer a run", fork: "fork", background: "background work", rollback: "roll back", models: "list models", usage: "report usage",
  readHistory: "read earlier messages", readSubagent: "read a sub-agent's messages", stopTask: "stop a background task", probe: "check the engine",
}));

/** What the host said it can do, in its own order. A word this app does not know is shown as it came. */
export const capabilityLine = (capabilities: string[]) =>
  capabilities.map((name, at) => {
    const word = CAPABILITY_WORDS.get(name) ?? plain(String(name), 40);
    return at === 0 ? word.charAt(0).toUpperCase() + word.slice(1) : word;
  }).join(", ");

/** The banner above a detail page when the host crashed or was refused. Empty when there is nothing to say. */
export function bannerLine(row: HostRow): string | null {
  if (row.state === "version_mismatch") {
    return `The engine host does not match this app version. The host speaks protocol ${row.protocol ?? "?"}; this app speaks protocol ${row.app_protocol ?? "?"}.`;
  }
  if (row.state === "crashed" || row.state === "restarting") {
    const threads = row.failed_threads ? ` ${plural(row.failed_threads, "thread", "threads")} that ${row.failed_threads === 1 ? "was" : "were"} running on it failed.` : "";
    return `The ${plain(row.name, 60)} engine host stopped${row.stopped_at ? ` at ${clock(row.stopped_at)}` : ""}.${threads} Your work in the project is kept.`;
  }
  return null;
}

/** Text the service or a model chose (an engine's name, an error): cut so no line can grow the page. React escapes it; it is never markup. */
export const plain = (text: string, max = 80): string => {
  const whole = String(text);
  return whole.length > max ? `${whole.slice(0, max)}…` : whole;
};

const CHIP_MAP = new Map<string, Chip>(Object.entries(CHIPS));

/** The chip for a state word. A word this app does not know (the service chose it) is Unknown, never a crash or a borrowed look. */
export const chipFor = (state: string): Chip => (typeof state === "string" ? CHIP_MAP.get(state) : undefined) ?? { label: "Unknown", tone: "plain" };

/** `3 h 4 min`, `38 min`, `45 s`: how long a host has run, as the Processes list writes it. */
export function runningFor(since: string | null | undefined, now: number): string {
  const start = since ? Date.parse(since) : Number.NaN;
  if (Number.isNaN(start)) return "";
  const seconds = Math.max(0, Math.round((now - start) / 1000));
  if (seconds < 60) return `${seconds} s`;
  const minutes = Math.floor(seconds / 60);
  return minutes < 60 ? `${minutes} min` : `${Math.floor(minutes / 60)} h ${minutes % 60} min`;
}

/** Hosts that are a process: running, restarting or stuck. One that is idle or never started is not listed. */
export const isProcess = (row: HostRow): boolean => row.state === "running" || row.state === "restarting" || row.state === "crashed" || row.state === "cant_start";

/** The line under a host in Maintenance › Processes. */
export function processLine(row: HostRow, now: number): string {
  switch (row.state) {
    case "running": {
      const long = runningFor(row.since, now);
      return `Running${long ? ` ${long}` : ""} · ${row.pooled ? `shared by every ${plain(row.name, 40)} thread` : `orbyn-engine-host ${plain(row.host_version ?? "", 20)}`}`.trimEnd();
    }
    case "restarting":
    case "crashed": {
      const restart = row.restart;
      return restart ? `Restarting · try ${restart.attempt} of ${restart.limit}${restart.next_try_seconds === null ? "" : ` · next try in ${restart.next_try_seconds} s`}` : row.restart_count ? `Restarting · restarted ${plural(row.restart_count, "time", "times")}` : "Restarting";
    }
    default: return row.detail ? `Can’t start · ${sentence(plain(row.detail, 120)).replace(/\.$/, "")}` : `Can’t start · stopped ${row.stops_in_window ?? 5} times in 5 minutes`;
  }
}

/** What Stop costs, in the words the Stop question uses. */
export const STOPPED_SENTENCE = "The engine host stopped. Your work in the project is kept; send your message again.";
