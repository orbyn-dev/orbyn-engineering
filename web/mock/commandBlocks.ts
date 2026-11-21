/**
 * MOCK of the command-blocks routes of docs/command-blocks.md (provisional), for tests and the dev-only preview
 * (`#/preview/command-blocks`), which installs it through `setBlocksApi`. The app's own default is no client at all: nothing here ships.
 * The scenario is the word after the route, as in `#/preview/command-blocks?failed`: `default`, `failed`, `running`, `long`,
 * `fullscreen`, `many`, `empty`, `error`. Output the mock serves includes markup and escape sequences on purpose: the blocks must draw
 * them as plain text.
 */
import { ApiFailure } from "../src/api/client.ts";
import type { BlockList, BlockOutputPage, BlockTerminal, CommandBlock, CommandBlocksApi } from "../src/features/panels/terminal/blocks/blocksContract.ts";

export const COMMAND_BLOCK_SCENARIOS = ["default", "failed", "running", "long", "fullscreen", "many", "empty", "error"] as const;
export type CommandBlockScenario = (typeof COMMAND_BLOCK_SCENARIOS)[number];

const LIMITS = { max_blocks: 500, max_block_bytes: 1_048_576, max_total_bytes: 8_388_608 };
const PAGE_LINES = 40;

interface Seed {
  block: CommandBlock;
  text: string;
}

const iso = (ms: number) => new Date(ms).toISOString();

function seeds(scenario: CommandBlockScenario, now: number): Seed[] {
  const done = (id: string, command: string | null, tookMs: number, exit: number | null, text: string, ago: number, extra: Partial<CommandBlock> = {}): Seed => ({
    block: {
      id, command, started_at: iso(now - ago), ended_at: iso(now - ago + tookMs), exit_code: exit, cwd: "/work/orbyn", output_bytes: text.length,
      truncated: false, dropped_lines: 0, fullscreen: false, ...extra,
    },
    text,
  });
  const status = done("b1", "git status", 100, 0, "On branch fix/login\nChanges not staged for commit:\n\tmodified: auth/redirect.ts\n\tmodified: auth/redirect.test.ts\n", 300_000);
  const failed = done(
    "b2", "npm test", 4_200, 1,
    " \u001b[32m✓\u001b[0m auth/redirect.test.ts (6)\n ✓ auth/session.test.ts (11)\n \u001b[31m✗\u001b[0m e2e/login.spec.ts › returns to the page after sign-in\n   Expected \"/settings\", received \"/home\"\n\n Tests  1 failed | 47 passed (48)\n",
    200_000,
  );
  const hostile = done("b3", "echo '<img src=x onerror=alert(1)>'", 80, 0, "<img src=x onerror=alert(1)>\n<script>alert(2)</script>\n\u001b]8;;http://example.invalid\u0007a link\u001b]8;;\u0007\nprogress 10%\rprogress 100%\n", 100_000);
  switch (scenario) {
    case "empty":
    case "error":
      return [];
    case "failed":
      return [status, failed];
    case "running":
      return [
        status,
        { block: { id: "r1", command: "npm run dev", started_at: iso(now - 42_000), ended_at: null, exit_code: null, cwd: "/work/orbyn", output_bytes: 120, truncated: false, dropped_lines: 0, fullscreen: false },
          text: "VITE v5.4.2 ready in 412 ms\n  Local: http://localhost:5173/\n  press h + enter to show help\n" },
      ];
    case "long": {
      const lines = Array.from({ length: 1_248 }, (_, index) => `14:02:${String(33 + (index % 20)).padStart(2, "0")} compiled src/module-${index + 1}.ts`).join("\n") + "\n";
      return [status, done("b4", "npm run build", 38_600, 0, lines, 60_000)];
    }
    case "fullscreen":
      return [status, done("f1", "vim notes.md", 38_000, 0, "", 120_000, { fullscreen: true, output_bytes: 0 })];
    case "many": {
      const many = Array.from({ length: 24 }, (_, index) =>
        done(`m${index + 1}`, index % 7 === 3 ? "npm test" : `echo step ${index + 1}`, 120 + index * 40, index % 7 === 3 ? 1 : 0, `step ${index + 1}\n`, 600_000 - index * 20_000));
      return [status, failed, hostile, ...many];
    }
    default:
      return [status, failed, hostile];
  }
}

export function createCommandBlocksMock(init: { delayMs?: number; scenario?: CommandBlockScenario } = {}): CommandBlocksApi & { writes: string[] } {
  const hash = typeof location !== "undefined" ? location.hash : "";
  const named = COMMAND_BLOCK_SCENARIOS.find((name) => hash.includes(`?${name}`));
  const scenario = init.scenario ?? named ?? "default";
  const now = Date.now();
  const all = seeds(scenario, now);
  const delay = () => new Promise((resolve) => setTimeout(resolve, init.delayMs ?? 0));
  const gone = (signal: AbortSignal) => {
    if (signal.aborted) throw new DOMException("Aborted", "AbortError");
  };
  return {
    writes: [],
    async list(_terminal: BlockTerminal, page: { before?: string }, signal: AbortSignal): Promise<BlockList> {
      await delay();
      gone(signal);
      if (scenario === "error") throw new ApiFailure("terminal_blocks_unavailable", "The service could not read this terminal.", 500);
      // The mock's newest page holds the last 20 blocks; the cursor `older` is the rest.
      const older = page.before !== undefined;
      const cut = Math.max(0, all.length - 20);
      return {
        blocks: all.slice(older ? 0 : cut).map((seed) => ({ ...seed.block })),
        next_before: older || cut === 0 ? null : "older",
        limits: LIMITS,
      };
    },
    async output(_terminal: BlockTerminal, blockId: string, page: { after?: string }, signal: AbortSignal): Promise<BlockOutputPage> {
      await delay();
      gone(signal);
      const seed = all.find((one) => one.block.id === blockId);
      if (!seed) throw new ApiFailure("not_found", "That block is gone.", 404);
      const lines = seed.text === "" ? [] : seed.text.replace(/\n$/, "").split("\n");
      const from = page.after === undefined ? 0 : Number(page.after);
      if (!Number.isInteger(from) || from < 0) throw new ApiFailure("bad_cursor", "That cursor is not one the service made.", 422);
      const chunk = lines.slice(from, from + PAGE_LINES);
      const end = from + chunk.length;
      return {
        text: chunk.length === 0 ? "" : `${chunk.join("\n")}\n`,
        next: end < lines.length ? String(end) : null,
        total_lines: seed.block.ended_at === null ? null : lines.length,
        dropped_lines: seed.block.dropped_lines,
      };
    },
  };
}
