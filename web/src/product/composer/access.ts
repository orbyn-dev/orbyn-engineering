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
  full_access: { label: "Full access", description: "Allow commands and edits without prompts.", icon: ShieldOff },
};

const MODES = Object.keys(ACCESS) as PermissionMode[];

/** The modes an engine offers, in menu order. An older service that does not say offers all. */
export function offeredModes(engine: Pick<EngineStatus, "capabilities"> | null): PermissionMode[] {
  const offered = engine?.capabilities.permission_modes;
  return offered ? MODES.filter((mode) => offered.includes(mode)) : MODES;
}

/** What a saved mode runs as: one the engine does not offer runs, and shows, Supervised. */
export function shownAccess(engine: Pick<EngineStatus, "capabilities"> | null, saved: PermissionMode): PermissionMode {
  return offeredModes(engine).includes(saved) ? saved : "supervised";
}
