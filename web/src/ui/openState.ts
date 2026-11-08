import { createContext, useCallback, useEffect, useState, type Dispatch, type SetStateAction, use } from "react";

/** Which thread's rows are around this component, or null where nothing should be remembered. */
export const OpenStateScope = createContext<string | null>(null);
/** The row a component sits in, so one id can repeat in different rows. */
export const OpenStatePrefix = createContext("");

const kept = new Map<string, Map<string, unknown>>();
const KEPT_THREADS = 8;

function bucket(scope: string): Map<string, unknown> {
  let known = kept.get(scope);
  if (known) {
    kept.delete(scope);
  } else {
    known = new Map();
  }
  kept.set(scope, known);
  for (const old of [...kept.keys()].slice(0, Math.max(0, kept.size - KEPT_THREADS))) kept.delete(old);
  return known;
}

const listeners = new Map<string, Set<() => void>>();
const listenKey = (scope: string, key: string) => `${scope}\u0000${key}`;
const notify = (scope: string, key: string) => listeners.get(listenKey(scope, key))?.forEach((listener) => listener());

/**
 * State that survives its row leaving the page: a windowed list unmounts rows that scroll far
 * away, and a fold or card the person opened must still be open when they come back.
 */
export function useOpenState<T>(id: string | null, initial: T): [T, Dispatch<SetStateAction<T>>] {
  const inScope = use(OpenStateScope);
  const prefix = use(OpenStatePrefix);
  const scope = id === null ? null : inScope;
  const key = `${prefix}/${id ?? ""}`;
  const [value, set] = useState<T>(() => (scope !== null && bucket(scope).has(key) ? (bucket(scope).get(key) as T) : initial));
  const update: Dispatch<SetStateAction<T>> = useCallback(
    (next) => {
      set((prev) => {
        const resolved = typeof next === "function" ? (next as (value: T) => T)(prev) : next;
        if (scope !== null) bucket(scope).set(key, resolved);
        return resolved;
      });
    },
    [scope, key],
  );
  // Another part of the thread (Expand all, Collapse all) can change what this row remembers.
  useEffect(() => {
    if (scope === null) return;
    const id = listenKey(scope, key);
    const listener = () => set(bucket(scope).has(key) ? (bucket(scope).get(key) as T) : initial);
    const set_ = listeners.get(id) ?? new Set();
    set_.add(listener);
    listeners.set(id, set_);
    return () => {
      set_.delete(listener);
      if (set_.size === 0) listeners.delete(id);
    };
    // `initial` only matters for a key nothing has written.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scope, key]);
  return [value, update];
}

/** Forgets what a thread's rows remembered; for tests. */
export const forgetOpenState = () => kept.clear();

/** Reads and sets what rows of this thread remember, by id: for a control that acts on many rows at once. */
export function useOpenControl() {
  const scope = use(OpenStateScope);
  const prefix = use(OpenStatePrefix);
  const peek = useCallback((id: string): unknown => (scope === null ? undefined : bucket(scope).get(`${prefix}/${id}`)), [scope, prefix]);
  const setMany = useCallback(
    (ids: readonly string[], value: unknown) => {
      if (scope === null) return;
      for (const id of ids) {
        bucket(scope).set(`${prefix}/${id}`, value);
        notify(scope, `${prefix}/${id}`);
      }
    },
    [scope, prefix],
  );
  return { peek, setMany };
}
