/**
 * Request lanes and shared reads.
 *
 * The desktop webview (WKWebView) opens at most 6 HTTP/1.1 connections per host to
 * the local service, and two stay busy for good with the shell and thread event
 * streams. A few slow reads (pull requests, models, limits) used to hold every
 * remaining connection, so 1 ms reads such as the project and thread lists waited
 * behind them. Slow reads now run in a lane of their own, capped so that at least
 * two connections stay free for everything else.
 */

/** Browser limit this budget is drawn from: 6 connections per host. */
export const CONNECTIONS_PER_HOST = 6;
/** Connections the two event streams hold for as long as the window is open. */
const STREAM_CONNECTIONS = 2;
/** Slow reads running at once: the rest queue, which leaves 2 of the 6 connections for interactive requests. */
export const SLOW_LANE_CAP = CONNECTIONS_PER_HOST - STREAM_CONNECTIONS - 2;

export type Lane = "slow" | "interactive";

/** Reads whose service work takes from a few hundred milliseconds to seconds. */
const SLOW_READS: readonly RegExp[] = [
  /^\/api\/pulls(\/|$)/,
  /^\/api\/forges(\/|$)/,
  /^\/api\/limits(\/|$)/,
  /^\/api\/usage(\/|$)/,
  /^\/api\/health\/dependencies$/,
  /^\/api\/workspace$/,
  /^\/api\/updates$/,
  /^\/api\/engines\/[^/]+\/(models|tools)$/,
  /^\/api\/projects\/[^/]+\/pulls(\/|$)/,
  /^\/api\/threads\/[^/]+\/(tools|commands|limits)$/,
];

/** Writes never wait behind reads; only a read can be slow enough to queue. */
export function laneFor(method: string, path: string): Lane {
  if (method !== "GET") return "interactive";
  const route = path.split("?")[0] ?? path;
  return SLOW_READS.some((pattern) => pattern.test(route)) ? "slow" : "interactive";
}

const abortError = () => new DOMException("The request was aborted.", "AbortError");

interface Waiter {
  start: () => void;
  reject: (error: unknown) => void;
}

class Limit {
  private active = 0;
  private readonly queue: Waiter[] = [];
  constructor(private readonly cap: number) {}

  /** Resolves with a release function once a slot is free, in arrival order. */
  acquire(signal?: AbortSignal): Promise<() => void> {
    if (signal?.aborted) return Promise.reject(abortError());
    return new Promise((resolve, reject) => {
      const waiter: Waiter = {
        start: () => {
          signal?.removeEventListener("abort", onAbort);
          this.active += 1;
          let released = false;
          resolve(() => {
            if (released) return;
            released = true;
            this.active -= 1;
            this.drain();
          });
        },
        reject,
      };
      const onAbort = () => {
        const at = this.queue.indexOf(waiter);
        if (at >= 0) this.queue.splice(at, 1);
        reject(abortError());
      };
      signal?.addEventListener("abort", onAbort, { once: true });
      this.queue.push(waiter);
      this.drain();
    });
  }

  private drain() {
    while (this.active < this.cap && this.queue.length > 0) this.queue.shift()?.start();
  }
}

const slowLane = new Limit(SLOW_LANE_CAP);

/** Runs `work` when its lane has room. A request aborted while it waits never starts. */
export async function inLane<T>(lane: Lane, signal: AbortSignal | undefined, work: () => Promise<T>): Promise<T> {
  if (lane === "interactive") return work();
  const release = await slowLane.acquire(signal);
  try {
    return await work();
  } finally {
    release();
  }
}

interface Flight {
  promise: Promise<unknown>;
  controller: AbortController;
  callers: number;
}

const flights = new Map<string, Flight>();

/**
 * Identical reads in flight share one network request. Each caller gets its own copy of
 * the result and its own abort; the shared request is aborted only once every caller has.
 */
export function shareRead<T>(key: string, signal: AbortSignal | undefined, start: (signal: AbortSignal) => Promise<T>): Promise<T> {
  if (signal?.aborted) return Promise.reject(abortError());
  let flight = flights.get(key);
  if (!flight) {
    const controller = new AbortController();
    const made: Flight = { promise: Promise.resolve(), controller, callers: 0 };
    made.promise = start(controller.signal).finally(() => {
      if (flights.get(key) === made) flights.delete(key);
    });
    // Callers handle the outcome; this keeps a fully abandoned flight from reporting an unhandled rejection.
    made.promise.catch(() => undefined);
    flights.set(key, made);
    flight = made;
  }
  const joined = flight;
  joined.callers += 1;
  return new Promise<T>((resolve, reject) => {
    const leave = () => signal?.removeEventListener("abort", onAbort);
    const onAbort = () => {
      leave();
      joined.callers -= 1;
      if (joined.callers === 0) {
        if (flights.get(key) === joined) flights.delete(key);
        joined.controller.abort();
      }
      reject(abortError());
    };
    signal?.addEventListener("abort", onAbort, { once: true });
    joined.promise.then(
      (value) => {
        leave();
        resolve(structuredClone(value) as T);
      },
      (error: unknown) => {
        leave();
        reject(error);
      },
    );
  });
}
