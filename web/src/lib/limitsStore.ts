/**
 * One shared memory of the engine limit reads, so every screen that shows them (the composer panel, the thread
 * details row, Usage) goes through the same 5-minute window. An automatic check (mount, thread open, a service
 * event) is answered from memory while the last good answer is under five minutes old; only a person pressing a
 * refresh control (`force`) goes out at once. There is no timer here: the window is a timestamp compared on each
 * read, so a screen unmounting has nothing to clear.
 */
import { AWAKE, registerRefresh, under } from "./dataRefresh";

export const LIMITS_STALE_MS = 5 * 60_000;
/** After a failed check, automatic reads wait this long before asking again. */
export const LIMITS_BACKOFF_MS = 30_000;

interface Entry {
  data?: unknown;
  /** When the last good answer arrived. */
  at?: number;
  failedAt?: number;
  error?: unknown;
  inflight?: Promise<unknown>;
  /** Bumped by every request that goes out; only the newest may store its answer. */
  generation: number;
  forced: boolean;
}

const entries = new Map<string, Entry>();

const entryOf = (key: string): Entry => {
  let entry = entries.get(key);
  if (!entry) {
    entry = { generation: 0, forced: false };
    entries.set(key, entry);
  }
  return entry;
};

/** The engine instance a read belongs to; a thread with no known engine keeps its own window. */
export const engineLimitsKey = (engine: string) => `engine:${engine}`;
export const limitsKey = (engine: string, threadId: string) => (engine ? engineLimitsKey(engine) : `thread:${threadId}`);

/** Milliseconds since the last good answer for this engine instance, or null when there is none. */
export function limitsAge(key: string): number | null {
  const at = entries.get(key)?.at;
  return at === undefined ? null : Date.now() - at;
}

/** Whether asking again would leave the five-minute window: no answer yet counts as old. */
export function limitsStale(key: string): boolean {
  const age = limitsAge(key);
  return age === null || age >= LIMITS_STALE_MS;
}

/**
 * What makes a read run again: a service `limits` event at once, and the app coming back into view only when the
 * shared answer is older than the window. A wake therefore never blanks and re-asks a figure that is still fresh.
 */
export const limitsRefreshOn = (matches: (what: string) => boolean, key: () => string) =>
  (what: string) => (what === AWAKE ? limitsStale(key()) : matches(what));

/** When the last good answer for this key arrived (epoch ms), or null when there is none: the label's "Checked …". */
export function limitsAnsweredAt(key: string): number | null {
  return entries.get(key)?.at ?? null;
}

/** Drops what is remembered for one key, so the next read goes out (for a read made outside the store). */
export function forgetLimits(key: string) {
  entries.delete(key);
}

/**
 * A read the browser abandoned because whoever asked left (a screen unmounting, React replaying an effect):
 * it never reached the service's answer. That is not a failed check, so it records no error and starts no
 * backoff; whoever still wants the figure asks again.
 */
const isCancellation = (error: unknown) => error instanceof DOMException && error.name === "AbortError";

/** Test seam: forget everything. */
export function resetLimitsStore() {
  entries.clear();
}

interface ReadOptions {
  force?: boolean;
}

/**
 * `load(force)` makes the real request. `force: true` is a person's refresh: it skips the window and the error
 * backoff, and replaces a slower request already on its way (the newest request wins).
 *
 * An abandoned read is not an answer: a screen leaving cancels the request it started, and React's replay of an
 * effect asks on the same shared request the first mount sent. The cancellation records nothing, and a check that
 * had joined the cancelled request asks once more as itself instead of being left without a figure.
 */
export function readLimits<T>(key: string, load: (force: boolean) => Promise<T>, options: ReadOptions = {}): Promise<T> {
  return readLimitsOnce(key, load, options, true);
}

/**
 * One attempt. `chase` is set for a check that may ask again when the request it joined is cancelled out from under
 * it; the retry itself does not chase, so a request cancelled twice is not chased further.
 */
function readLimitsOnce<T>(key: string, load: (force: boolean) => Promise<T>, options: ReadOptions, chase: boolean): Promise<T> {
  const entry = entryOf(key);
  const force = options.force === true;
  const now = Date.now();
  if (!force) {
    // One request in flight per instance: a second automatic check joins it.
    if (entry.inflight) return joined(entry.inflight as Promise<T>, () => readLimitsOnce(key, load, options, false), chase);
    if (entry.at !== undefined && now - entry.at < LIMITS_STALE_MS && (entry.failedAt === undefined || entry.failedAt < entry.at)) {
      return Promise.resolve(entry.data as T);
    }
    if (entry.failedAt !== undefined && now - entry.failedAt < LIMITS_BACKOFF_MS) {
      // A failure is not a fresh answer: show the last good one meanwhile, or the same error, until the backoff ends.
      return entry.at !== undefined ? Promise.resolve(entry.data as T) : Promise.reject(entry.error);
    }
  } else if (entry.inflight && entry.forced) {
    // Two manual presses in a row share the one forced request.
    return joined(entry.inflight as Promise<T>, () => readLimitsOnce(key, load, options, false), chase);
  }
  const id = ++entry.generation;
  entry.forced = force;
  const request: Promise<T> = load(force).then(
    (data) => {
      if (id === entry.generation) {
        entry.data = data;
        entry.at = Date.now();
        entry.failedAt = undefined;
        entry.error = undefined;
        entry.inflight = undefined;
        return data;
      }
      // A newer request replaced this one: its answer is the truth, this one is dropped.
      return (entry.at !== undefined ? entry.data : data) as T;
    },
    (error: unknown) => {
      if (id === entry.generation) {
        entry.inflight = undefined;
        // A cancelled check never happened: no failedAt, so the next automatic check is not held back.
        if (isCancellation(error)) throw error;
        entry.failedAt = Date.now();
        entry.error = error;
        // The last good answer stays on screen through a failed automatic check; a manual one reports its error.
        if (entry.at !== undefined && !force) return entry.data as T;
      }
      throw error;
    },
  );
  entry.inflight = request;
  return request;
}

/**
 * A check waiting on a request another caller started. Only a cancellation leaves it without an answer (a failure is
 * the answer for everyone who joined), so it asks once more through the store: that joins whatever request replaced
 * the cancelled one, or starts its own.
 */
function joined<T>(request: Promise<T>, retry: () => Promise<T>, chase: boolean): Promise<T> {
  if (!chase) return request;
  return request.catch((error: unknown) => {
    if (!isCancellation(error)) throw error;
    return retry();
  });
}

// The service sends `limits` when what a read would answer has changed (another account signed in, a window reset).
// That is a reason to ask again, so the next read after it goes out whatever its age. Registered once, with the store.
registerRefresh(under("limits"), () => resetLimitsStore());
