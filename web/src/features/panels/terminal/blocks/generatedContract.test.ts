import { expectTypeOf, it } from "vitest";
import type { BlockLimits as WireBlockLimits } from "../../../../generated/BlockLimits";
import type { BlockList as WireBlockList } from "../../../../generated/BlockList";
import type { BlockOutputPage as WireBlockOutputPage } from "../../../../generated/BlockOutputPage";
import type { CommandBlock as WireCommandBlock } from "../../../../generated/CommandBlock";
import type { BlockLimits, BlockList, BlockOutputPage, CommandBlock } from "./blocksContract";

/** The hand-written blocks contract must keep accepting everything the service sends; the web only caps and cleans it. */
it("accepts every shape the generated service types describe", () => {
  expectTypeOf<WireCommandBlock>().toMatchTypeOf<CommandBlock>();
  expectTypeOf<WireBlockLimits>().toMatchTypeOf<BlockLimits>();
  expectTypeOf<WireBlockList>().toMatchTypeOf<BlockList>();
  expectTypeOf<WireBlockOutputPage>().toMatchTypeOf<BlockOutputPage>();
});
