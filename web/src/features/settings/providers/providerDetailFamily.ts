import type { EngineStatus } from "../../../api/types";

/** Beta detail pages exist only for these families, including named instances. */
export function hasProviderDetail(engine: EngineStatus): boolean {
  return ["claude", "codex", "opencode"].includes(engine.instance_of ?? engine.id);
}
