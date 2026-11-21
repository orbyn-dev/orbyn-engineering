/**
 * Command blocks in the terminal drawer (parity G17, Warp-style W1), built hidden. The service keeps no per-command output yet;
 * the routes it needs are written in docs/command-blocks.md. `blocksApi()` answers `null` in a production build, so the drawer
 * draws no blocks and sends nothing: the terminal is unchanged. The mock lives in web/mock/commandBlocks.ts and is installed only by the dev-only preview page (`#/preview/command-blocks`).
 */

export interface CommandBlock {
  /** Service-made and opaque. Keep it as a map key, never as an object key or in a path. */
  id: string;
  /** The command line as the service redacted and capped it, or null when the shell sent none. Text, never markup. */
  command: string | null;
  started_at: string;
  /** Null while the command runs. */
  ended_at: string | null;
  /** From OSC 133 D. Null while running, or when the shell sent no code. */
  exit_code: number | null;
  cwd: string | null;
  output_bytes: number;
  /** Bytes were dropped from the top of this block's output. */
  truncated: boolean;
  dropped_lines: number;
  /** A program that used the alternate screen: its output is not kept. */
  fullscreen: boolean;
}

export interface BlockLimits {
  max_blocks: number;
  max_block_bytes: number;
  max_total_bytes: number;
}

export interface BlockList {
  /** Oldest first. */
  blocks: CommandBlock[];
  /** Opaque cursor for the page before this one, or null at the start. */
  next_before: string | null;
  limits: BlockLimits;
}

export interface BlockOutputPage {
  /** Plain text, oldest first. The web strips escapes again before it draws or copies. */
  text: string;
  /** Opaque cursor for the next page, or null on the last. */
  next: string | null;
  /** All kept lines, or null while the command still runs. */
  total_lines: number | null;
  dropped_lines: number;
}

export interface BlockTerminal {
  threadId: string;
  /** `main` for the thread's own shell. */
  terminalId: string;
}

export interface CommandBlocksApi {
  /** The newest page, or the page before `before`. */
  list(terminal: BlockTerminal, page: { before?: string }, signal: AbortSignal): Promise<BlockList>;
  /** One page of a block's output. Rejects with `cursor_expired` when `after` points into text the service dropped. */
  output(terminal: BlockTerminal, blockId: string, page: { after?: string }, signal: AbortSignal): Promise<BlockOutputPage>;
}

let current: CommandBlocksApi | null = null;

export const blocksApi = (): CommandBlocksApi | null => current;

/** Tests and the dev-only preview install a client here; `null` puts back the production default, which sends nothing. */
export function setBlocksApi(next: CommandBlocksApi | null): void {
  current = next;
}
