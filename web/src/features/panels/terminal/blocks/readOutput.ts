import { ApiFailure } from "../../../../api/client";
import type { BlockTerminal, CommandBlocksApi } from "./blocksContract";
import { capTail, OUTPUT_MAX_CHARS, stripTerminalText } from "./blockText";

/** Pages read for one block at most: a service that never ends its cursors cannot keep the web reading. */
const MAX_PAGES = 64;

export interface BlockOutput {
  text: string;
  total_lines: number | null;
  dropped_lines: number;
  /** The web kept only the tail, because the output was longer than it holds. */
  capped: boolean;
}

/** Reads every page of a block's output, cleaned of escapes and capped. A cursor the service dropped is started again from the top once. */
export async function readBlockOutput(api: CommandBlocksApi, terminal: BlockTerminal, blockId: string, signal: AbortSignal): Promise<BlockOutput> {
  for (let restart = 0; ; restart += 1) {
    try {
      return await readPages(api, terminal, blockId, signal);
    } catch (failure) {
      if (restart === 0 && failure instanceof ApiFailure && failure.code === "cursor_expired") continue;
      throw failure;
    }
  }
}

async function readPages(api: CommandBlocksApi, terminal: BlockTerminal, blockId: string, signal: AbortSignal): Promise<BlockOutput> {
  let text = "";
  let after: string | undefined;
  let total: number | null = null;
  let dropped = 0;
  for (let page = 0; page < MAX_PAGES; page += 1) {
    const got = await api.output(terminal, blockId, after === undefined ? {} : { after }, signal);
    if (signal.aborted) throw new DOMException("Aborted", "AbortError");
    text += got.text;
    total = got.total_lines;
    dropped = got.dropped_lines;
    if (got.next === null) break;
    after = got.next;
  }
  const clean = capTail(stripTerminalText(text), OUTPUT_MAX_CHARS);
  return { text: clean.text, total_lines: total, dropped_lines: dropped, capped: clean.cut };
}
