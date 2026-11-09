import type { AuthState, RunState } from "../api/types";

export const runStateLabels: Record<RunState, string> = {
  admitted: "Queued",
  starting: "Starting",
  running: "Running",
  awaiting_approval: "Waiting for you",
  completed: "Completed",
  failed: "Failed",
  cancelled: "Cancelled",
  interrupted: "Interrupted",
};

export const authLabels: Record<AuthState, string> = {
  logged_in: "Signed in",
  logged_out: "Signed out",
  unknown: "Login unknown",
  not_applicable: "No login needed",
};

const units: [Intl.RelativeTimeFormatUnit, number][] = [
  ["day", 86_400],
  ["hour", 3_600],
  ["minute", 60],
];

/** "3 min ago" style; the absolute value goes in `title`/`dateTime`. */
export function relativeTime(iso: string, now = Date.now()): string {
  const seconds = (now - new Date(iso).getTime()) / 1000;
  if (Number.isNaN(seconds)) return "";
  if (Math.abs(seconds) < 60) return "Just now";
  const format = new Intl.RelativeTimeFormat(undefined, { numeric: "auto", style: "short" });
  for (const [unit, size] of units) {
    if (Math.abs(seconds) >= size) return format.format(-Math.round(seconds / size), unit);
  }
  return "";
}

/** "7m", "2h", "3d": the sidebar's age column. */
export function compactAge(iso: string, now = Date.now()): string {
  const seconds = Math.max(0, (now - new Date(iso).getTime()) / 1000);
  if (Number.isNaN(seconds)) return "";
  if (seconds < 60) return "now";
  if (seconds < 3_600) return `${Math.floor(seconds / 60)}m`;
  if (seconds < 86_400) return `${Math.floor(seconds / 3_600)}h`;
  return `${Math.floor(seconds / 86_400)}d`;
}

/** "18s", "3m 05s": how long something has been going. */
export function elapsed(fromIso: string, toMs: number): string {
  const seconds = Math.max(0, Math.round((toMs - new Date(fromIso).getTime()) / 1000));
  if (Number.isNaN(seconds)) return "";
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ${String(seconds % 60).padStart(2, "0")}s`;
  return `${Math.floor(minutes / 60)}h ${String(minutes % 60).padStart(2, "0")}m`;
}

export function clockTime(iso: string, format: "locale" | "12-hour" | "24-hour" = "locale"): string {
  const date = new Date(iso);
  const hour12 = format === "locale" ? undefined : format === "12-hour";
  // No leading zero on a 12-hour clock (a 24-hour one keeps two digits), and AM or PM in capitals.
  const twelve = format === "12-hour" || (format === "locale" && /^h1[12]$/.test(new Intl.DateTimeFormat(undefined, { hour: "numeric" }).resolvedOptions().hourCycle ?? ""));
  return Number.isNaN(date.getTime())
    ? ""
    : date
        .toLocaleTimeString(undefined, { hour: twelve ? "numeric" : "2-digit", minute: "2-digit", second: "2-digit", hour12 })
        .replace(/\b(am|pm)\b/gi, (mark) => mark.toUpperCase());
}

export function fullTime(iso: string): string {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? iso : date.toLocaleString();
}

/** Pretty-prints JSON tool arguments; anything else is returned untouched. */
export function prettyJson(text: string): string {
  try {
    return JSON.stringify(JSON.parse(text), null, 2);
  } catch {
    return text;
  }
}

/** Turns a free-form name into the slug the server expects for ids and worktrees. */
export function slugify(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 48);
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const DAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const pad = (value: number) => String(value).padStart(2, "0");
const clock24 = (at: Date) => `${pad(at.getHours())}:${pad(at.getMinutes())}`;

/** "Today 10:12", "Yesterday 16:40", "3 Nov 09:02": a time in a list, by the 24-hour clock. */
export function dayStamp(iso: string, now = new Date()): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return iso;
  const midnight = (date: Date) => new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
  const days = Math.round((midnight(now) - midnight(at)) / 86_400_000);
  const clock = clock24(at);
  return days === 0 ? `Today ${clock}` : days === 1 ? `Yesterday ${clock}` : `${at.getDate()} ${MONTHS[at.getMonth()]} ${clock}`;
}

/** "Mon 9 Nov 07:00": a run's start, which can be days from now or long gone. */
export function weekdayStamp(iso: string): string {
  const at = new Date(iso);
  return Number.isNaN(at.getTime()) ? iso : `${DAYS[at.getDay()]} ${at.getDate()} ${MONTHS[at.getMonth()]} ${clock24(at)}`;
}

/** "next Mon 9 Nov": a day named in a sentence. */
export function weekdayDate(iso: string): string {
  const at = new Date(iso);
  return Number.isNaN(at.getTime()) ? "" : `${DAYS[at.getDay()]} ${at.getDate()} ${MONTHS[at.getMonth()]}`;
}

/** "45 s", "6 min", "2 h 10 min": how long a finished piece of work took. */
export function tookWords(ms: number): string {
  const minutes = Math.round(ms / 60_000);
  if (ms < 60_000) return `${Math.max(0, Math.round(ms / 1000))} s`;
  if (minutes < 60) return `${minutes} min`;
  return minutes % 60 === 0 ? `${minutes / 60} h` : `${Math.floor(minutes / 60)} h ${minutes % 60} min`;
}

/** Whole megabytes, gigabytes with one decimal: the sizes the settings frames write. */
export function sizeWords(bytes: number): string {
  return bytes >= 1e9 ? `${(bytes / 1e9).toFixed(1)} GB` : `${Math.max(bytes > 0 ? 1 : 0, Math.round(bytes / 1e6))} MB`;
}

// Durations, ages and sizes. The writers below differ on purpose (rounding, padding, units),
// so each keeps its own name; pick by what the screen says, not by what looks closest.

/** A tool call row: tenths under 10 s, then 41s, 1m 02s. Never hours (75 min reads 75m 00s). */
export function callDuration(ms: number): string {
  const tenths = Math.max(0, Math.round(ms / 100));
  if (tenths < 100) return `${Math.floor(tenths / 10)}.${tenths % 10}s`;
  const seconds = Math.round(ms / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  return `${minutes}m ${String(seconds % 60).padStart(2, "0")}s`;
}

/** A finished turn: 45s, 1m 2s, 2h 5m. Zero parts are dropped (1m, 2h). */
export function workDuration(ms: number): string {
  const seconds = Math.max(0, Math.round(ms / 1000));
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return seconds % 60 ? `${minutes}m ${seconds % 60}s` : `${minutes}m`;
  return minutes % 60 ? `${Math.floor(minutes / 60)}h ${minutes % 60}m` : `${Math.floor(minutes / 60)}h`;
}

/** The time left on a limit wait, rounded up: 2m 14s, or 1h 5m (minutes unpadded after an hour). */
export function waitLeft(ms: number): string {
  const seconds = Math.max(Math.ceil(ms / 1000), 0);
  if (seconds >= 3600) return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`;
  if (seconds >= 60) return `${Math.floor(seconds / 60)}m ${String(seconds % 60).padStart(2, "0")}s`;
  return `${seconds}s`;
}

/** "2 min ago", "3 hr ago", "1 d ago", from epoch milliseconds: how stale a list is. */
export function agoWords(at: number, now: number): string {
  const minutes = Math.max(0, Math.floor((now - at) / 60_000));
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  return hours < 24 ? `${hours} hr ago` : `${Math.floor(hours / 24)} d ago`;
}

/** A process uptime: 59s, 5m, 2h. Whole units only. */
export const uptime = (seconds: number) =>
  seconds < 60 ? `${seconds}s` : seconds < 3600 ? `${Math.floor(seconds / 60)}m` : `${Math.floor(seconds / 3600)}h`;

/** Mebibytes with one decimal: 1.5 MiB. */
export const mebibytes = (bytes: number) => `${(bytes / 1_048_576).toFixed(1)} MiB`;

/** Byte sizes a person reads: 512 B, 1.5 KB, 12 MB. */
export function formatBytes(size: number): string {
  if (!Number.isFinite(size) || size < 0) return "0 B";
  if (size < 1024) return `${size} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = size / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  // One decimal only when the size has one: "12 MB", not "12.0 MB".
  return `${Number.isInteger(value) ? String(value) : value.toFixed(1)} ${units[unit]}`;
}

/** A token count with three significant figures: 950, 1.23K, 79.2M, 5.79B. */
export function formatTokens(value: number): string {
  const steps = [
    [1e12, "T"],
    [1e9, "B"],
    [1e6, "M"],
    [1e3, "K"],
  ] as const;
  for (const [size, unit] of steps) {
    if (value >= size) {
      const scaled = value / size;
      return `${scaled >= 100 ? Math.round(scaled) : Number(scaled.toPrecision(3))}${unit}`;
    }
  }
  return String(Math.round(value));
}
