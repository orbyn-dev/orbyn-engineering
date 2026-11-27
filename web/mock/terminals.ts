/**
 * The mock's terminals: the routes of `orbynd` (`crates/orbynd/src/routes/terminal.rs`) that list, open, split and close a
 * thread's terminals, run a project script in one, and read a terminal's command blocks (docs/command-blocks.md). No shell runs:
 * a test or a screen seeds what the service would list, and the mock keeps the list as the service does, so a closed terminal is
 * gone from the next read.
 */
import type { BlockClear } from "../src/generated/BlockClear.ts";
import type { BlockList } from "../src/generated/BlockList.ts";
import type { CommandBlock } from "../src/generated/CommandBlock.ts";
import type { ProjectFileView } from "../src/generated/ProjectFileView.ts";
import type { ProjectScript } from "../src/generated/ProjectScript.ts";
import type { TerminalInfo } from "../src/generated/TerminalInfo.ts";
import type { MockResponse } from "./api.ts";

const LIMITS = { max_blocks: 500, max_block_bytes: 1_048_576, max_total_bytes: 8_388_608 };

const notFound = (): MockResponse => ({ status: 404, body: { code: "not_found", message: "Not found." } });

export function createTerminalsMock() {
  let lists = new Map<string, TerminalInfo[]>();
  let scripts = new Map<string, ProjectScript[]>();
  let blocks = new Map<string, { blocks: CommandBlock[]; outputs: Record<string, string> }>();
  let opened = 0;

  const list = (threadId: string) => lists.get(threadId) ?? [];

  return {
    reset() {
      lists = new Map();
      scripts = new Map();
      blocks = new Map();
      opened = 0;
    },
    /** What the service lists for a thread from now on. */
    seed(threadId: string, terminals: TerminalInfo[]) {
      lists.set(threadId, terminals);
    },
    /** The project's scripts for a thread, by name and command. */
    setScripts(threadId: string, entries: { name: string; command: string; icon?: string | null }[]) {
      scripts.set(threadId, entries.map((entry) => ({
        id: null, icon: null, run_on_worktree_create: false, async: false, run_on_settle: false, preview_url: null, auto_open_preview: false, ...entry,
      })));
    },
    /** The command blocks a terminal keeps, and the output text of each by block id. */
    setBlocks(threadId: string, terminalId: string, entries: CommandBlock[], outputs: Record<string, string> = {}) {
      blocks.set(`${threadId}/${terminalId}`, { blocks: entries, outputs });
    },
    /** Answers the terminal and script routes, or undefined for any other path. `seg` is the decoded path. */
    handle(method: string, seg: string[], body: unknown): MockResponse | undefined {
      if (seg[0] !== "api" || seg[1] !== "threads" || !seg[2]) return undefined;
      const threadId = seg[2];
      const data = (body ?? {}) as { split_of?: string; direction?: string; name?: string; id?: string };
      if (seg[3] === "scripts" && !seg[4]) {
        const known = scripts.get(threadId);
        if (!known) return undefined;
        if (method === "GET") return { status: 200, body: { path: null, file: null, problem: null, scripts: known } satisfies ProjectFileView };
        if (method === "POST") {
          const script = known.find((entry) => (data.id ? entry.id === data.id : entry.name === data.name));
          if (!script) return notFound();
          opened += 1;
          const made: TerminalInfo = {
            id: `t-run${opened}`, thread_id: threadId, title: script.name, group: `g-run${opened}`, direction: null, alive: true, exit_code: null,
            clients: 1, script: script.name, created_at: new Date().toISOString(),
          };
          lists.set(threadId, [...list(threadId), made]);
          return { status: 201, body: made };
        }
        return undefined;
      }
      if (seg[3] !== "terminals") return undefined;
      if (!seg[4]) {
        if (method === "GET") return { status: 200, body: list(threadId) };
        if (method !== "POST") return undefined;
        const other = data.split_of === undefined ? undefined : list(threadId).find((entry) => entry.id === data.split_of);
        if (data.split_of !== undefined && !other) return notFound();
        opened += 1;
        const made: TerminalInfo = {
          id: `t-new${opened}`, thread_id: threadId, title: `zsh ${opened}`, group: other?.group ?? `g-new${opened}`, direction: data.direction ?? null,
          alive: true, exit_code: null, clients: 1, script: null, created_at: new Date().toISOString(),
        };
        lists.set(threadId, [...list(threadId), made]);
        return { status: 201, body: made };
      }
      const terminalId = seg[4];
      if (!seg[5]) {
        if (method !== "DELETE") return undefined;
        if (!list(threadId).some((entry) => entry.id === terminalId)) return notFound();
        lists.set(threadId, list(threadId).filter((entry) => entry.id !== terminalId));
        return { status: 204 };
      }
      if (seg[5] === "blocks" && method === "DELETE") {
        if (!list(threadId).some((entry) => entry.id === terminalId)) return notFound();
        const recorded = blocks.get(`${threadId}/${terminalId}`);
        if (!recorded) return { status: 200, body: { cleared: 0, kept_running: 0 } satisfies BlockClear };
        const running = recorded.blocks.filter((entry) => entry.ended_at === null);
        const outputs = Object.fromEntries(running.map((entry) => [entry.id, recorded.outputs[entry.id] ?? ""]));
        blocks.set(`${threadId}/${terminalId}`, { blocks: running, outputs });
        return { status: 200, body: { cleared: recorded.blocks.length - running.length, kept_running: running.length > 0 ? 1 : 0 } satisfies BlockClear };
      }
      if (seg[5] === "blocks" && method === "GET") {
        const kept = blocks.get(`${threadId}/${terminalId}`);
        if (!kept) return notFound();
        if (!seg[6]) return { status: 200, body: { blocks: kept.blocks, next_before: null, limits: LIMITS } satisfies BlockList };
        if (seg[7] === "output") {
          const block = kept.blocks.find((entry) => entry.id === seg[6]);
          if (!block) return notFound();
          const text = kept.outputs[block.id] ?? "";
          return { status: 200, body: { text, next: null, total_lines: block.ended_at === null ? null : text.split("\n").filter(Boolean).length, dropped_lines: block.dropped_lines } };
        }
      }
      return undefined;
    },
  };
}
