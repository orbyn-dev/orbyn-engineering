import { vi } from "vitest";

/** One held request: its reply reaches the page at `release()`. */
export interface HeldRequest {
  release(): void;
  signal: AbortSignal | undefined;
}

/**
 * Makes the service slow to answer one request at a time: each matching request is answered at once by the service,
 * as of the moment it is sent, but the reply reaches the page only at that request's `release()`. `held` lists the
 * matching requests in the order the page sent them.
 */
export function holdEach(method: string, path: RegExp | string): HeldRequest[] {
  const service = fetch;
  const held: HeldRequest[] = [];
  vi.stubGlobal("fetch", vi.fn(async (url: string, init?: RequestInit) => {
    const same = typeof path === "string" ? String(url) === path : path.test(String(url));
    if (!same || (init?.method ?? "GET") !== method) return service(url, init);
    let release: () => void = () => undefined;
    const gate = new Promise<void>((resolve) => { release = resolve; });
    held.push({ release, signal: init?.signal ?? undefined });
    const reply = await service(url, init);
    await gate;
    return reply;
  }));
  return held;
}
