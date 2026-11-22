/** The capabilities field the service may omit on an older answer. */
export interface ManagedEngine {
  capabilities?: { managed?: boolean } | null;
}

/**
 * Rule 8: true only when the service says Orbyn cannot gate this engine's actions.
 * A missing field is unknown, never unmanaged, so an older service labels nothing.
 */
export function isUnmanaged(engine: ManagedEngine | null | undefined): boolean {
  return engine?.capabilities?.managed === false;
}
