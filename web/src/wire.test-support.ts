import { vi } from "vitest";

/** One request the page sent through the stubbed `fetch` since `installMockFetch()`. */
export interface WireCall {
  method: string;
  path: string;
  body?: unknown;
}

/** Every request sent through the stubbed `fetch`, in order, as the service received it. The update and thread event streams are left out. */
export function wire(): WireCall[] {
  return vi.mocked(fetch).mock.calls
    .filter(([url]) => !/^\/api\/(engines\/update-events|threads\/[^/?]+\/events)/.test(String(url)))
    .map(([url, init]) => ({
      method: init?.method ?? "GET",
      path: String(url),
      ...(typeof init?.body === "string" ? { body: JSON.parse(init.body) as unknown } : {}),
    }));
}

/** The requests that change something: everything but reads. */
export const wireWrites = (): WireCall[] => wire().filter((call) => call.method !== "GET");

/**
 * Makes the service slow to answer: the first `times` matching requests are answered at once by the service, but the reply
 * reaches the page only at `release()`. A request counts in `wire()` from the moment the page sends it, and `signals`
 * holds the abort signal each held one carried.
 */
export function holdFetch(method: string, path: RegExp | string, options: { times?: number } = {}) {
  const service = fetch;
  const signals: AbortSignal[] = [];
  let release: () => void = () => undefined;
  const gate = new Promise<void>((resolve) => { release = resolve; });
  let left = options.times ?? Infinity;
  vi.stubGlobal("fetch", vi.fn(async (url: string, init?: RequestInit) => {
    const same = typeof path === "string" ? String(url) === path : path.test(String(url));
    if (same && (init?.method ?? "GET") === method && left > 0) {
      left -= 1;
      if (init?.signal) signals.push(init.signal);
      const reply = await service(url, init);
      await gate;
      return reply;
    }
    return service(url, init);
  }));
  return { release, signals };
}
