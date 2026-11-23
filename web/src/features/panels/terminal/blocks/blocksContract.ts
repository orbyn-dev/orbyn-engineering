/**
 * Command blocks in the terminal drawer (parity G17, Warp-style W1). The service keeps the blocks and serves them through the two
 * Local reads of docs/command-blocks.md; `blocksClient.ts` is the production client. A shell that emits no marks answers an empty
 * list, and the drawer then draws the terminal exactly as it does without blocks. The mock lives in web/mock/commandBlocks.ts and is
 * installed only by the dev-only preview page (`#/preview/command-blocks`); tests install their own client with `setBlocksApi`.
 */

import { realBlocksApi } from "./blocksClient";
import type { CommandBlocksApi } from "./blocksTypes";

export type {
  BlockLimits,
  BlockList,
  BlockOutputPage,
  BlockTerminal,
  CommandBlock,
  CommandBlocksApi,
} from "./blocksTypes";

let current: CommandBlocksApi = realBlocksApi;

export const blocksApi = (): CommandBlocksApi => current;

/** Tests and the dev-only preview install a client here; `null` puts back the production client. */
export function setBlocksApi(next: CommandBlocksApi | null): void {
  current = next ?? realBlocksApi;
}
