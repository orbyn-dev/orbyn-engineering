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

/**
 * A slow read that has not answered by now is abandoned, so two hung forge reads cannot
 * hold both slots and stall every slow read in the app.
 */
export const LANE_TIMEOUT_MS = 30_000;

export type Lane = "slow" | "interactive";

/** Reads whose service work takes from a few hundred milliseconds to seconds. */
const SLOW_READS: readonly RegExp[] = [
  /^\/api\/pulls(\/|$)/,
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

  reset() {
    this.active = 0;
    this.queue.length = 0;
  }

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

/**
 * Runs `work` when its lane has room. A request aborted while it waits never starts, and a
 * slow one that outlasts `LANE_TIMEOUT_MS` is aborted, rejects with `timedOut()` and frees its slot.
 */
export async function inLane<T>(
  lane: Lane,
  signal: AbortSignal | undefined,
  work: (signal: AbortSignal | undefined) => Promise<T>,
  timedOut: () => Error,
): Promise<T> {
  if (lane === "interactive") return work(signal);
  const release = await slowLane.acquire(signal);
  const controller = new AbortController();
  const follow = () => controller.abort();
  signal?.addEventListener("abort", follow, { once: true });
  let timer: ReturnType<typeof setTimeout> | undefined;
  const expiry = new Promise<never>((_, reject) => {
    timer = setTimeout(() => {
      reject(timedOut());
      controller.abort();
    }, LANE_TIMEOUT_MS);
  });
  try {
    const running = work(controller.signal);
    running.catch(() => undefined);
    return await Promise.race([running, expiry]);
  } finally {
    clearTimeout(timer);
    signal?.removeEventListener("abort", follow);
    release();
  }
}

/**
 * An identical read is joined only while the first is this young. Reads made together, such as
 * two components mounting in one render, share one request. A read made later, after a key
 * press or an event the client cannot see as a write, may need newer server state than a
 * request that has been in flight since, so it goes out on its own.
 */
export const SHARE_WINDOW_MS = 100;

interface Flight {
  startedAt: number;
  promise: Promise<unknown>;
  controller: AbortController;
  callers: number;
  /** The first caller gets the answer itself; only the others get copies. */
  handedOut: boolean;
}

const flights = new Map<string, Flight>();

/**
 * Identical reads in flight share one network request. Each caller gets its own copy of
 * the result and its own abort; the shared request is aborted only once every caller has.
 */
export function shareRead<T>(key: string, signal: AbortSignal | undefined, start: (signal: AbortSignal) => Promise<T>): Promise<T> {
  if (signal?.aborted) return Promise.reject(abortError());
  let flight = flights.get(key);
  if (flight && Date.now() - flight.startedAt > SHARE_WINDOW_MS) flight = undefined;
  if (!flight) {
    const controller = new AbortController();
    const made: Flight = { startedAt: Date.now(), promise: Promise.resolve(), controller, callers: 0, handedOut: false };
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
        const own = joined.handedOut ? (structuredClone(value) as T) : (value as T);
        joined.handedOut = true;
        resolve(own);
      },
      (error: unknown) => {
        leave();
        reject(error);
      },
    );
  });
}

/**
 * A write may change what an identical read answers, so reads sent before it must not be
 * joined by reads made after it. The reads in flight finish for their own callers.
 */
export function forgetFlights() {
  flights.clear();
}

/** Test only: drops every shared read and every queued or running slow read. */
export function resetLanesForTests() {
  flights.clear();
  slowLane.reset();
}
