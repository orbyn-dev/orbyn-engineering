import type { EngineStatus } from "../../api/types";
import type { HostRow } from "./contract";
import { plain, restartLine, type Chip } from "./hostText";

/**
 * Why an engine has no models or can't run, in the words the Engineer frames give (`enginehost-picker-*`, `enginehost-gaps-*`).
 * One engine says one thing and offers the one action that can change it. The service's names and messages are cut and shown as
 * text; nothing here reads a credential.
 */
export type GapKind =
  | "starting" | "restarting" | "cant_start" | "mismatch" | "sdk_missing" | "unavailable" | "expired" | "not_found";

export type GapAction = "try_again" | "update" | "sign_in_again" | "check_again" | "open";

export interface Gap {
  kind: GapKind;
  title: string;
  sentence: string;
  chip: Chip;
  action: GapAction | null;
}

/** The program the person installs themselves, as the sentence names it: `claude`, `cursor`, `opencode`, `muse`. */
export const programOf = (engine: EngineStatus): string => plain((engine.instance_of ?? engine.id).split(":")[0] ?? engine.id, 24);

const NAME = (engine: EngineStatus) => plain(engine.name, 60);

/**
 * What stops an engine on its own, whatever the host is doing: not installed, the SDK missing from this build, the sign-in
 * expired. `host` is the engine's host row, or null when the service reports none.
 */
export function engineGap(engine: EngineStatus, host: HostRow | null): Gap | null {
  const name = NAME(engine);
  const program = programOf(engine);
  if (!engine.installed) {
    return { kind: "not_found", title: "Not installed", sentence: `${program} isn’t installed on this computer.`, chip: { label: "Not found", tone: "plain" }, action: "check_again" };
  }
  if (host?.sdk_missing) {
    return host.has_driver
      ? { kind: "sdk_missing", title: "SDK missing in this build", sentence: `This build does not include the ${name} SDK. ${name} runs through its ${program} program.`, chip: { label: "SDK missing", tone: "plain" }, action: "open" }
      : { kind: "unavailable", title: "SDK missing in this build", sentence: `The ${name} SDK is not included in this build. ${name} can’t run here.`, chip: { label: "Unavailable", tone: "err" }, action: null };
  }
  if (host?.last_error?.kind === "auth") {
    return { kind: "expired", title: "Sign in again", sentence: `Your sign-in expired. Sign in with ${program}.`, chip: { label: "Expired", tone: "err" }, action: "sign_in_again" };
  }
  return null;
}

/** The engine picker's row for an engine: a gap, or the state of its host when that is not ready. Null means show the models. */
export function pickerGap(engine: EngineStatus, host: HostRow | null): Gap | null {
  const own = engineGap(engine, host);
  if (own) return own;
  if (!host) return null;
  switch (host.state) {
    case "starting":
      return { kind: "starting", title: "Starting the engine host", sentence: "Its models appear when it is ready. It has 10 seconds; the first start after an update has 30.", chip: { label: "Starting", tone: "info", spin: true }, action: null };
    case "restarting":
    case "crashed":
      return { kind: "restarting", title: "Restarting the engine host", sentence: `${restartLine(host)}. Your work in the project is kept.`, chip: { label: "Restarting", tone: "info", spin: true }, action: null };
    case "cant_start":
      return { kind: "cant_start", title: "Can’t start", sentence: `The engine host stopped ${host.stops_in_window ?? 5} times in 5 minutes.${host.last_error ? ` Last error: ${plain(host.last_error.message, 200).replace(/\.$/, "")}.` : ""}`, chip: { label: "Can’t start", tone: "err" }, action: host.can_retry ? "try_again" : null };
    case "version_mismatch":
      return { kind: "mismatch", title: "Version mismatch", sentence: "The engine host does not match this app version.", chip: { label: "Version mismatch", tone: "err" }, action: "update" };
    default:
      return null;
  }
}

/** True when sending on this engine has nowhere to go: its host is unavailable and nothing else can run it. */
export const cannotSend = (host: HostRow | null): boolean => host?.state === "cant_start" && host.has_driver !== true;

/** True when the host is unavailable but a Rust driver keeps Send working. */
export const sendsWithoutHost = (host: HostRow | null): boolean => host?.state === "cant_start" && host.has_driver === true;
