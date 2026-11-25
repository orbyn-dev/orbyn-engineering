import { ApiFailure, NETWORK_ERROR, authHeaders, failureFrom, request } from "../../../../api/client";
import { serviceUrl } from "../../../../api/origin";
import type { BlockClear as WireBlockClear } from "../../../../generated/BlockClear";
import type { BlockLimits as WireBlockLimits } from "../../../../generated/BlockLimits";
import type { BlockList as WireBlockList } from "../../../../generated/BlockList";
import type { BlockOutputPage as WireBlockOutputPage } from "../../../../generated/BlockOutputPage";
import type { CommandBlock as WireCommandBlock } from "../../../../generated/CommandBlock";
import type { BlockLimits, BlockList, BlockOutputPage, CommandBlock, CommandBlocksApi } from "./blocksTypes";

const q = encodeURIComponent;

/**
 * What a service without the list route keeps: nothing. A `404` on the list means the thread or terminal is gone, or the
 * service predates the routes; it is answered like an empty list, so the drawer draws no blocks and no error.
 */
const NO_LIMITS: BlockLimits = { max_blocks: 0, max_block_bytes: 0, max_total_bytes: 0 };

/** The generated contract types may say `bigint`; JSON delivers plain numbers, so read them as numbers. */
const count = (value: number | bigint): number => Number(value);
const countOrNull = (value: number | bigint | null): number | null => (value === null ? null : Number(value));

function toBlock(wire: WireCommandBlock): CommandBlock {
  return {
    ...wire,
    exit_code: countOrNull(wire.exit_code),
    output_bytes: count(wire.output_bytes),
    dropped_lines: count(wire.dropped_lines),
  };
}

function toLimits(wire: WireBlockLimits): BlockLimits {
  return {
    max_blocks: count(wire.max_blocks),
    max_block_bytes: count(wire.max_block_bytes),
    max_total_bytes: count(wire.max_total_bytes),
  };
}

function toList(wire: WireBlockList): BlockList {
  return { blocks: wire.blocks.map(toBlock), next_before: wire.next_before, limits: toLimits(wire.limits) };
}

function toPage(wire: WireBlockOutputPage): BlockOutputPage {
  return {
    text: wire.text,
    next: wire.next,
    total_lines: countOrNull(wire.total_lines),
    dropped_lines: count(wire.dropped_lines),
  };
}

async function read<T>(path: string, signal: AbortSignal): Promise<T> {
  let response: Response;
  try {
    response = await fetch(serviceUrl(path), {
      method: "GET",
      credentials: "same-origin",
      signal,
      headers: authHeaders(),
    });
  } catch (error) {
    if (error instanceof DOMException && error.name === "AbortError") throw error;
    throw new ApiFailure(NETWORK_ERROR, "Can't reach the Orbyn service.", 0);
  }
  if (!response.ok) throw await failureFrom(response);
  return (await response.json()) as T;
}

/**
 * The production client for docs/command-blocks.md: the two Local reads and the Person clear. A read carries the session
 * header and no person key, and every path segment and cursor is encoded. A `410` rejects with `cursor_expired`, which
 * `readBlockOutput` uses to start the block again from the top; a `422 bad_cursor` and a failed output page surface to
 * the card unchanged. The clear goes through `request`, so the person key reaches that route.
 */
export const realBlocksApi: CommandBlocksApi = {
  quietWhenEmpty: true,

  async list(terminal, page, signal) {
    const before = page.before === undefined ? "" : `?before=${q(page.before)}`;
    const path = `/api/threads/${q(terminal.threadId)}/terminals/${q(terminal.terminalId)}/blocks${before}`;
    try {
      return toList(await read<WireBlockList>(path, signal));
    } catch (error) {
      if (error instanceof ApiFailure && error.status === 404) return { blocks: [], next_before: null, limits: NO_LIMITS };
      throw error;
    }
  },

  async output(terminal, blockId, page, signal) {
    const after = page.after === undefined ? "" : `?after=${q(page.after)}`;
    const path = `/api/threads/${q(terminal.threadId)}/terminals/${q(terminal.terminalId)}/blocks/${q(blockId)}/output${after}`;
    try {
      return toPage(await read<WireBlockOutputPage>(path, signal));
    } catch (error) {
      // Keep the contract's code whatever a service puts in the body: the drawer's restart depends on it.
      if (error instanceof ApiFailure && error.status === 410 && error.code !== "cursor_expired") {
        throw new ApiFailure("cursor_expired", error.message, error.status, error.detail);
      }
      throw error;
    }
  },
};

/**
 * The Person-class clear of docs/command-blocks.md, through the app's `request` so it carries the person key. It drops this
 * terminal's finished blocks and their kept output; a running block stays and its output goes on growing. The answer says how
 * many finished blocks went and whether one stayed.
 */
export function clearTerminalBlocks(terminal: BlockTerminal, signal?: AbortSignal): Promise<WireBlockClear> {
  const path = `/api/threads/${q(terminal.threadId)}/terminals/${q(terminal.terminalId)}/blocks`;
  return request<WireBlockClear>("DELETE", path, undefined, signal);
}
