/** An active editor may defer navigation; otherwise routing is unchanged. */
export type NavigationResume = (before?: () => void) => void;
export type NavigationGuardOwner = { dispose(): void; cancelAttempt(): void };
let active: { token: symbol; generation: number; hash?: string; ask(resume: NavigationResume): void } | null = null;
export function registerNavigationGuard(ask: (resume: NavigationResume) => void, hash?: string): NavigationGuardOwner {
  const token = Symbol("editor navigation"); active = { token, generation: 0, hash, ask };
  return {
    dispose() { if (active?.token === token) active = null; },
    cancelAttempt() { if (active?.token === token) active.generation += 1; },
  };
}
/** Whether an editor still holds navigation: after its Discard it does not, even before it unmounts. */
export const navigationGuarded = (): boolean => active !== null;
export function deferNavigation(resume: () => void): boolean {
  if (!active) return false;
  const owner = active, generation = ++owner.generation;
  owner.ask((before) => {
    if (active?.token !== owner.token || active.generation !== generation) return;
    active = null; before?.(); resume();
  });
  return true;
}

/** Hold the current editor route even if React reads before the native hashchange event. */
export function navigationSnapshot(hash: string): string { return active?.hash ?? hash; }
