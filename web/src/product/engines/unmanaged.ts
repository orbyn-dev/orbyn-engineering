/** The capabilities and names the wording needs; an older answer may omit any of them. */
export interface ManagedEngine {
  id?: string;
  name?: string;
  capabilities?: { managed?: boolean } | null;
}

/** The one engine whose own sandbox makes the old sentence true. Every other engine says Orbyn cannot ask. */
const CURSOR = "cursor";

/**
 * Rule 8: true only when the service says Orbyn cannot gate this engine's actions.
 * A missing field is unknown, never unmanaged, so an older service labels nothing.
 */
export function isUnmanaged(engine: ManagedEngine | null | undefined): boolean {
  return engine?.capabilities?.managed === false;
}

/** The two ways the sentence starts: Cursor's own clause, and Orbyn's full sentence for every other engine. */
function askBeforeParts(engineName: string): { cursor: string; other: string } {
  return {
    cursor: `${engineName} does not ask before acting`,
    other: `Orbyn cannot ask before ${engineName} acts.`,
  };
}

/**
 * The one sentence that says why an engine acts without Orbyn's ask. Cursor keeps its own words, because it has no approvals;
 * every other engine says what is true for Orbyn, which cannot ask for it.
 */
export function askBeforeText(engineId: string, engineName: string): string {
  const parts = askBeforeParts(engineName);
  return engineId === CURSOR ? `${parts.cursor}.` : parts.other;
}

/** The same sentence for an engine object, for labels and hover reasons. */
export function unmanagedReason(engine: ManagedEngine | null | undefined): string {
  return askBeforeText(engine?.id ?? "", engine?.name ?? engine?.id ?? "");
}

/**
 * The reason followed by its consequence: Cursor keeps its comma sentence, and every other engine starts a new sentence after
 * its full stop. The wording for the engine stays here; the caller names the consequence.
 */
export function askBeforeFollowOn(engineId: string, engineName: string, cursorTail: string, otherTail: string): string {
  const parts = askBeforeParts(engineName);
  return engineId === CURSOR ? `${parts.cursor}, ${cursorTail}` : `${parts.other} ${otherTail}`;
}

/** The notice's full text: Cursor adds its own sandbox sentence, and every other engine stops after Orbyn's reason. */
export function unmanagedNoticeText(engineId: string, engineName: string): string {
  const parts = askBeforeParts(engineName);
  return engineId === CURSOR ? `${parts.cursor}. Its own sandbox applies.` : parts.other;
}
