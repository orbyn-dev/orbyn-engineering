/**
 * MOCK of the Cursor thread routes (per-project notice read and acknowledge, one-turn override), which are not on the service yet.
 * It is never called against the real service: the app's own client reports nothing until the routes exist, and only a test or the
 * dev-only preview (`#/preview/cursor-thread`) installs this through `setCursorThreadApi`. It holds one flag and a date per project and engine;
 * there is no field for a credential.
 */
import type { CursorThreadApi, NoticeState } from "../src/features/cursorthread/contract.ts";

export function createCursorThreadMock(options: { delayMs?: number; acknowledged?: boolean; engines?: string[] } = {}): CursorThreadApi & {
  writes: { acknowledge: { project_id: string; engine: string }[]; override: string[] };
} {
  const wait = () => new Promise<void>((done) => setTimeout(done, options.delayMs ?? 0));
  const engines = new Set(options.engines ?? ["cursor"]);
  const state = new Map<string, NoticeState>();
  const key = (projectId: string, engine: string) => `${projectId}/${engine}`;
  const read = (project_id: string, engine: string): NoticeState =>
    state.get(key(project_id, engine)) ?? { engine, project_id, acknowledged: options.acknowledged ?? false, acknowledged_at: options.acknowledged ? new Date().toISOString() : null };
  const writes = { acknowledge: [] as { project_id: string; engine: string }[], override: [] as string[] };
  return {
    writes,
    async notice(projectId, engine) {
      await wait();
      return engines.has(engine) ? read(projectId, engine) : null;
    },
    async acknowledge(projectId, engine) {
      await wait();
      writes.acknowledge.push({ project_id: projectId, engine });
      // Acknowledging twice changes nothing: the first date stays.
      const next = state.get(key(projectId, engine)) ?? { engine, project_id: projectId, acknowledged: true, acknowledged_at: new Date().toISOString() };
      state.set(key(projectId, engine), next);
      return next;
    },
    async sendWithoutApprovals(threadId) {
      await wait();
      writes.override.push(threadId);
      return { id: "run-override" };
    },
  };
}
