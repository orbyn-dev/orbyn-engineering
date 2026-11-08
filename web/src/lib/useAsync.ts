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
 * again; only a key with nothing kept starts as `loading`.
 */
const startState = <T,>(key: string): AsyncState<T> => {
  const data = readKept<T>(key);
  return data === undefined
    ? { status: "loading", data: null, error: null, refreshing: true }
    : { status: "ready", data, error: null, refreshing: true };
};

export function useAsync<T>(key: string, load: (signal: AbortSignal) => Promise<T>) {
  const [state, setState] = useState<AsyncState<T>>(() => startState<T>(key));
  const keyRef = useRef(key);
  useLayoutEffect(() => {
    keyRef.current = key;
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
          writeKept(asked, data);
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
      setState(startState<T>(key));
    }
    reload();
    return () => {
      generation.current += 1;
      controller.current?.abort();
    };
  }, [key, reload]);

  const setData = useCallback((data: T) => {
    generation.current += 1;
    writeKept(keyRef.current, data);
    setState({ status: "ready", data, error: null, refreshing: false });
  }, []);

  return { ...state, reload, setData };
}
