import { Lock, PenLine, ShieldOff, Sparkles, type LucideIcon } from "lucide-react";
import type { EngineStatus, PermissionMode } from "../../api/types";

export const ACCESS: Record<PermissionMode, { label: string; description: string; icon: LucideIcon }> = {
  supervised: { label: "Supervised", description: "Ask before commands and file changes.", icon: Lock },
  auto_edits: {
    label: "Auto-accept edits",
    description: "Auto-approve edits, ask before other actions.",
    icon: PenLine,
  },
  auto: {
    label: "Auto",
    description: "Supported engines approve routine actions; others still ask.",
    icon: Sparkles,
  },
  full_access: { label: "Full access", description: "Allow commands and edits without prompts in this thread. Crew mode decides for crew workers.", icon: ShieldOff },
};

const MODES = Object.keys(ACCESS) as PermissionMode[];

/**
 * GUESS (service slice not merged): an engine that never asks before acting lists one mode, spelled `no_approvals`, which the
 * generated `PermissionMode` does not name yet. It is read from the engine's own list, never from the engine's name.
 */
const NO_APPROVALS = "no_approvals";

/** The modes an engine that cannot ask refuses at run start: Supervised and Auto edits. Auto and Full access stay as they are. */
const REFUSED_WITHOUT_ASKING: PermissionMode[] = ["supervised", "auto_edits"];

/** Whether this engine's own list says it never asks before acting. */
function neverAsks(engine: Pick<EngineStatus, "capabilities"> | null): boolean {
  const offered: readonly string[] | undefined = engine?.capabilities.permission_modes;
  return offered?.includes(NO_APPROVALS) ?? false;
}

/** The modes a thread on this engine cannot run in, in menu order: drawn dimmed with the reason, never hidden. */
export function refusedModes(engine: Pick<EngineStatus, "capabilities"> | null): PermissionMode[] {
  const offered: readonly string[] = engine?.capabilities.permission_modes ?? [];
  return neverAsks(engine) ? MODES.filter((mode) => REFUSED_WITHOUT_ASKING.includes(mode) && !offered.includes(mode)) : [];
}

/** The one sentence that says why, in the app's own mode names. */
export function refusalSentence(engineName: string, refused: PermissionMode[]): string {
  const names = refused.map((mode) => ACCESS[mode].label);
  const list = names.length > 1 ? `${names.slice(0, -1).join(", ")} and ${names.at(-1)}` : (names[0] ?? "");
  return `${engineName} does not ask before acting, so ${list} ${names.length > 1 ? "are" : "is"} not available here.`;
}

/** The modes an engine offers, in menu order. An older service that does not say offers all. */
export function offeredModes(engine: Pick<EngineStatus, "capabilities"> | null): PermissionMode[] {
  const offered = engine?.capabilities.permission_modes;
  if (!offered) return MODES;
  if (neverAsks(engine)) return MODES.filter((mode) => !refusedModes(engine).includes(mode));
  return MODES.filter((mode) => offered.includes(mode));
}

/**
 * What a saved mode runs as: one the engine does not offer runs, and shows, Supervised. An engine that refuses instead of
 * falling back shows the saved mode as it is, so what the person sees is what the next send will be refused for.
 */
export function shownAccess(engine: Pick<EngineStatus, "capabilities"> | null, saved: PermissionMode): PermissionMode {
  if (neverAsks(engine)) return saved;
  return offeredModes(engine).includes(saved) ? saved : "supervised";
}
