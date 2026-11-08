import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { ApiFailure } from "../api/client";
import { readKept, writeKept } from "./keptData";

export type AsyncState<T> =
  | { status: "loading"; data: null; error: null; refreshing: boolean }
  | { status: "error"; data: T | null; error: ApiFailure; refreshing: boolean }
  | { status: "ready"; data: T; error: null; refreshing: boolean };

export function toFailure(error: unknown): ApiFailure {
  return error instanceof ApiFailure
    ? error
    : new ApiFailure("unexpected", "Something went wrong.", 0);
}

const isAbort = (error: unknown) => error instanceof DOMException && error.name === "AbortError";

/**
 * One read owned by a screen. `key` is the resource identity: changing it drops
 * the old data. A result is applied only while its generation is current, so a
 * slow earlier response can never overwrite a later one. A key read before shows
 * its last good data at once as `ready` with `refreshing` true, and the read runs
 * again; only a key with nothing kept starts as `loading`. Keeping is opt-in
 * (`keep: true`) for the reads a screen wants to show again at once; files, diffs,
 * documents and anything that must be fresh before it is acted on never keep.
 */
export interface AsyncOptions {
  keep?: boolean;
}

const startState = <T,>(key: string, keep: boolean): AsyncState<T> => {
  const data = keep ? readKept<T>(key) : undefined;
  return data === undefined
    ? { status: "loading", data: null, error: null, refreshing: true }
    : { status: "ready", data, error: null, refreshing: true };
};

export function useAsync<T>(key: string, load: (signal: AbortSignal) => Promise<T>, options: AsyncOptions = {}) {
  const keep = options.keep === true;
  const [state, setState] = useState<AsyncState<T>>(() => startState<T>(key, keep));
  const keyRef = useRef(key);
  const keepRef = useRef(keep);
  useLayoutEffect(() => {
    keyRef.current = key;
    keepRef.current = keep;
  });
  const generation = useRef(0);
  const controller = useRef<AbortController | null>(null);
  const loader = useRef(load);
  useLayoutEffect(() => {
    loader.current = load;
  });

  const reload = useCallback(() => {
    const id = ++generation.current;
    const asked = keyRef.current;
    controller.current?.abort();
    const abort = new AbortController();
    controller.current = abort;
    setState((previous) => ({ ...previous, refreshing: true }));
    return loader.current(abort.signal).then(
      (data) => {
        if (id === generation.current) {
          if (keepRef.current) writeKept(asked, data);
          setState({ status: "ready", data, error: null, refreshing: false });
        }
      },
      (error: unknown) => {
        if (id !== generation.current || isAbort(error)) return;
        setState((previous) => ({
          status: "error",
          data: previous.data,
          error: toFailure(error),
          refreshing: false,
        }));
      },
    );
  }, []);

  const firstKey = useRef(key);
  useEffect(() => {
    if (firstKey.current !== key) {
      firstKey.current = key;
      setState(startState<T>(key, keepRef.current));
    }
    reload();
    return () => {
      generation.current += 1;
      controller.current?.abort();
    };
  }, [key, reload]);

  // Bound to the key it was made for: a mutation that resolves after the screen moved to
  // another key must not put its answer under that key or on its screen.
  const setData = useCallback(
    (data: T) => {
      if (keep) writeKept(key, data);
      if (key !== keyRef.current) return;
      generation.current += 1;
      setState({ status: "ready", data, error: null, refreshing: false });
    },
    [key, keep],
  );

  return { ...state, reload, setData };
}
