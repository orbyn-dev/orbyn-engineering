import { Loader2 } from "lucide-react";
import { traceTool } from "../../../../product/timeline/toolTrace";
import { commandText } from "../../toolCalls";
import { BlockHead, CopyButton, Dot, FooterText } from "./BlockParts";
import { InspectItem } from "./InspectItem";
import { OutputBlock } from "./OutputBlock";
import type { BlockContext } from "./types";

/** A shell command: the command, its output, then exit code and time. */
export function commandBlock({ item, running, cwd, duration, exit, failed, stopped }: BlockContext) {
  const command = traceTool(item).command;
  return (
    <div className="cb">
      <BlockHead counted lead="$" tools={command ? <CopyButton value={command} label="Copy command" /> : null}>
        {commandText(item)}
      </BlockHead>
      <OutputBlock item={item} running={running} />
      <div className="cf">
        <FooterText>
          {running ? (
            <>
              <Loader2 size={12} className="spin" aria-hidden /> Running
            </>
          ) : stopped ? (
            stopped
          ) : exit !== null ? (
            <span className={failed ? "er" : undefined}>exit {exit}</span>
          ) : null}
          {duration ? (
            <>
              {running || stopped || exit !== null ? <Dot /> : null}
              {duration}
            </>
          ) : null}
          {cwd ? (
            <>
              <Dot />
              {cwd}
            </>
          ) : null}
        </FooterText>
        <InspectItem item={item} />
      </div>
    </div>
  );
}
