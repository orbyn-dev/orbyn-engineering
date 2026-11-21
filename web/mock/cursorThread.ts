/**
 * MOCK of the Cursor thread routes (notice read, notice acknowledge, one-turn override), which are not merged on the service yet.
 * It is never called against the real service: the app's own client reports nothing until the routes exist, and only a test or the
 * dev-only preview (`#/preview/cursor-thread`) installs this through `setCursorThreadApi`. It holds one flag and a date per engine;
 * there is no field for a credential.
 */
import type { CursorThreadApi, NoticeState } from "../src/features/cursorthread/contract.ts";

export function createCursorThreadMock(options: { delayMs?: number; acknowledged?: boolean; engines?: string[] } = {}): CursorThreadApi & {
  writes: { acknowledge: string[]; override: string[] };
} {
  const wait = () => new Promise<void>((done) => setTimeout(done, options.delayMs ?? 0));
  const engines = new Set(options.engines ?? ["cursor"]);
  const state = new Map<string, NoticeState>();
  const read = (engine: string): NoticeState => state.get(engine) ?? { engine, acknowledged: options.acknowledged ?? false, acknowledged_at: options.acknowledged ? new Date().toISOString() : null };
  const writes = { acknowledge: [] as string[], override: [] as string[] };
  return {
    writes,
    async notice(engine) {
      await wait();
      return engines.has(engine) ? read(engine) : null;
    },
    async acknowledge(engine) {
      await wait();
      writes.acknowledge.push(engine);
      const next = { engine, acknowledged: true, acknowledged_at: new Date().toISOString() };
      state.set(engine, next);
      return next;
    },
    async sendWithoutApprovals(threadId) {
      await wait();
      writes.override.push(threadId);
      return { id: "run-override" };
    },
  };
}
