import type { ReactNode } from "react";
import { callDuration } from "../../../../lib/format";
import { parsed, toolFailed } from "../../../../lib/timeline/toolTrace";
import type { ToolItem } from "../../../../lib/timeline/types";
import { useUi } from "../../../../state/ui";
import { useThreadOfTimeline } from "../../timelineThread";
import { callKind, callPath, listEntries, shownPath, type CallState } from "../../toolCalls";
import { Dot, FooterText } from "./BlockParts";
import { commandBlock } from "./CommandBlock";
import { createBlock, editBlock, readBlock } from "./FileBlocks";
import { GenericBlock } from "./GenericBlock";
import { ListBlock } from "./ListBlock";
import { fetchBlock, mcpBlock, subagentBlock } from "./RemoteBlocks";
import { SearchBlock } from "./SearchBlock";
import type { BlockContext } from "./types";

function stopNote(state: CallState): ReactNode {
  return state === "stopped" ? "Stopped" : null;
}

export function Block({ item, running, state, ms, cwd }: { item: ToolItem; running: boolean; state: CallState; ms: number | null; cwd: string | null }) {
  const kind = callKind(item);
  const origin = { threadId: useThreadOfTimeline() ?? "", cwd: cwd ?? "" };
  const revealFile = useUi((s) => s.revealFile);
  const revealDiff = useUi((s) => s.revealDiff);
  const showInBrowser = useUi((s) => s.showInBrowser);
  const body = parsed(item.args);
  const duration = ms === null ? null : callDuration(ms);
  const exit = item.exitCode;
  const failed = toolFailed(item);
  const out = item.result ?? item.output;
  const path = callPath(item);
  const shown = path ? shownPath(path, cwd) : null;
  const stopped = stopNote(state);
  const timing = (...parts: ReactNode[]) => (
    <FooterText>
      {[...parts, duration].filter(Boolean).map((part, at) => (
        <span key={at}>
          {at > 0 ? <Dot /> : null}
          {part}
        </span>
      ))}
    </FooterText>
  );

  const ctx: BlockContext = {
    item,
    running,
    cwd,
    origin,
    body,
    duration,
    exit,
    failed,
    out,
    path,
    shown,
    stopped,
    revealFile,
    revealDiff,
    showInBrowser,
    timing,
  };

  switch (kind) {
    case "command":
      return commandBlock(ctx);
    case "read":
      return readBlock(ctx);
    case "edit":
      return editBlock(ctx);
    case "create":
      return createBlock(ctx);
    case "search":
      return <SearchBlock item={item} duration={duration} />;
    case "list": {
      const entries = listEntries(out);
      return (
        <ListBlock item={item} entries={entries} duration={duration} onOpen={path ? () => revealFile(path, null, origin) : null} shown={shown} />
      );
    }
    case "fetch":
      return fetchBlock(ctx);
    case "mcp":
      return mcpBlock(ctx);
    case "subagent":
      return subagentBlock(ctx);
    default:
      return <GenericBlock item={item} running={running} duration={duration} />;
  }
}
