/**
 * Opt-in single-flight for the reads a screen can start from more than one place at once.
 *
 * A request marked `share` joins an identical one already in flight (same method, path, body and
 * extra headers) instead of sending its own; every caller still gets the answer, or the same
 * failure, as if it had sent one. Writes never join (see `request`), a caller that asks for a
 * fresh answer never joins, and a request is abandoned only once every caller has aborted.
 */

/**
 * Reads share only while the first of them is this young. Readers made together (two components
 * mounting in one render) join; a read made later may need newer server state than a request
 * that has been in flight since — and one that never settles must not trap every later read.
 */
export const JOIN_WINDOW_MS = 100;

interface Flight {
  promise: Promise<unknown>;
  controller: AbortController;
  startedAt: number;
  callers: number;
  /** Two separate requests answer with two objects; only the first caller gets the one the flight holds. */
  handedOut: boolean;
}

const flights = new Map<string, Flight>();

const abortError = () => new DOMException("The request was aborted.", "AbortError");

/** The identical-request key: method, URL (which carries the read's query scope), body and extra headers. */
export function sharedKey(method: string, path: string, body: unknown, headers?: Record<string, string>): string {
  return [method, path, body === undefined ? "" : JSON.stringify(body), headers === undefined ? "" : JSON.stringify(headers)].join("\n");
}

/** Reads one key at a time: callers made together share the request the first of them sent. */
export function sharedRead<T>(key: string, start: (signal: AbortSignal) => Promise<T>, signal?: AbortSignal): Promise<T> {
  if (signal?.aborted) return Promise.reject(abortError());
  let flight = flights.get(key);
  if (flight && Date.now() - flight.startedAt > JOIN_WINDOW_MS) flight = undefined;
  if (!flight) {
    const made: Flight = { promise: Promise.resolve(), controller: new AbortController(), startedAt: Date.now(), callers: 0, handedOut: false };
    made.promise = start(made.controller.signal).finally(() => {
      if (flights.get(key) === made) flights.delete(key);
    });
    // Callers handle the outcome; this keeps a flight every caller left from reporting an unhandled rejection.
    made.promise.catch(() => undefined);
    // Replaces a flight older than the window, whose remaining callers still get its answer.
    flights.set(key, made);
    flight = made;
  }
  return join(flight, key, signal);
}

function join<T>(flight: Flight, key: string, signal?: AbortSignal): Promise<T> {
  flight.callers += 1;
  return new Promise<T>((resolve, reject) => {
    const leave = () => signal?.removeEventListener("abort", onAbort);
    const onAbort = () => {
      leave();
      flight.callers -= 1;
      // The request is kept for the callers still waiting; with none left it is abandoned.
      if (flight.callers === 0) {
        if (flights.get(key) === flight) flights.delete(key);
        flight.controller.abort();
      }
      reject(abortError());
    };
    signal?.addEventListener("abort", onAbort, { once: true });
    flight.promise.then(
      (value) => {
        leave();
        const own = flight.handedOut ? copyOf(value) : value;
        flight.handedOut = true;
        resolve(own as T);
      },
      (error: unknown) => {
        leave();
        reject(error);
      },
    );
  });
}

/** Callers of one request do not share its mutable objects, exactly as callers of two requests would not. */
function copyOf<T>(value: T): T {
  try {
    return structuredClone(value);
  } catch {
    return value;
  }
}

/**
 * A write may change what an identical read answers, so reads sent before it are not joined
 * by reads made after it. The reads already in flight finish for their own callers.
 */
export function forgetSharedReads(): void {
  flights.clear();
}

/** Test only: drops every read in flight. */
export function resetSharedReadsForTests(): void {
  flights.clear();
}
