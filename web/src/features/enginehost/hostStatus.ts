import { ApiFailure, request } from "../../api/client";
import type { EngineHostCrash } from "../../generated/EngineHostCrash";
import type { EngineHostStatus } from "../../generated/EngineHostStatus";
import type { HostCrash, HostRow } from "./contract";

/** Display names for the families the service names. A family not here is shown as the service sent it, cut. */
const FAMILY_NAMES = new Map<string, string>(Object.entries({
  claude: "Claude Code", codex: "Codex", cursor: "Cursor", opencode: "OpenCode", muse: "Muse", acp: "ACP agents", pi: "Pi",
}));

const CAP = 200;
const cut = (text: unknown, max = CAP): string => {
  const whole = typeof text === "string" ? text : "";
  return whole.length > max ? `${whole.slice(0, max)}…` : whole;
};
const optionalText = (text: unknown): string | undefined => (typeof text === "string" && text !== "" ? cut(text) : undefined);
const optionalNumber = (value: unknown): number | undefined => (typeof value === "number" && Number.isFinite(value) ? value : undefined);

export const familyName = (family: string): string => FAMILY_NAMES.get(family) ?? cut(family, 60);

/** A crash as the service sent it, with every text cut. The stderr tail is already capped by the core; it is cut again here. */
function readCrash(crash: unknown): EngineHostCrash | null {
  if (typeof crash !== "object" || crash === null) return null;
  const raw = crash as Partial<EngineHostCrash>;
  if (typeof raw.at !== "string") return null;
  return {
    exit_code: optionalNumber(raw.exit_code),
    signal: optionalNumber(raw.signal),
    at: raw.at,
    error_kind: raw.error_kind as EngineHostCrash["error_kind"],
    message: cut(raw.message),
    stderr_tail: cut(raw.stderr_tail, 1024),
  };
}

/** One family's status as the row the screens draw. A state word this app does not know passes through and is drawn as Unknown. */
export function toHostRow(status: EngineHostStatus): HostRow | null {
  if (typeof status !== "object" || status === null || typeof status.family !== "string" || status.family === "") return null;
  const crash = readCrash(status.last_crash);
  return {
    id: status.family,
    name: familyName(status.family),
    state: status.state,
    host_version: optionalText(status.host_version) ?? null,
    restart_count: optionalNumber(status.restart_count) ?? 0,
    last_crash: crash,
    protocol: optionalNumber(status.protocol_host) ?? null,
    app_protocol: optionalNumber(status.protocol_app) ?? null,
    not_used: status.not_used ?? null,
    detail: optionalText(status.detail) ?? null,
    action: optionalText(status.action) ?? null,
    stopped_at: crash?.at ?? null,
    last_error: crash ? { message: crash.message, causes: [] } : null,
    // Only a family that gave up waits for the person; a crashed host is already restarting by itself.
    can_retry: status.state === "cant_start",
  };
}

export function toHostCrash(status: EngineHostStatus): HostCrash | null {
  const crash = readCrash(status?.last_crash);
  if (!crash || typeof status.family !== "string") return null;
  return { id: `${status.family}:${crash.at}`, family: status.family, at: crash.at, message: crash.message, error_kind: crash.error_kind,
    exit_code: crash.exit_code, signal: crash.signal, stderr_tail: crash.stderr_tail };
}

/** `GET /api/engine-host`. 404 (an older service) is nothing reported: `null`, never an error. */
export async function readEngineHost(signal?: AbortSignal): Promise<EngineHostStatus[] | null> {
  try {
    const answer = await request<EngineHostStatus[]>("GET", "/api/engine-host", undefined, signal);
    return Array.isArray(answer) ? answer : null;
  } catch (failure) {
    if (failure instanceof ApiFailure && failure.status === 404) return null;
    throw failure;
  }
}

export async function hostRows(signal?: AbortSignal): Promise<HostRow[] | null> {
  const statuses = await readEngineHost(signal);
  return statuses === null ? null : statuses.flatMap((status) => toHostRow(status) ?? []);
}

export async function hostCrashes(signal?: AbortSignal): Promise<HostCrash[]> {
  const statuses = await readEngineHost(signal);
  return (statuses ?? []).flatMap((status) => toHostCrash(status) ?? []);
}
