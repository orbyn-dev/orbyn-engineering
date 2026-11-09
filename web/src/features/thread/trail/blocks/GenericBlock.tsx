import { prettyJson } from "../../../../lib/format";
import type { ToolItem } from "../../../../lib/timeline/types";
import { FooterText } from "./BlockParts";
import { InspectItem } from "./InspectItem";
import { OutputBlock } from "./OutputBlock";

/** What a call with no drawn block shows: its arguments, then its output. */
export function GenericBlock({ item, running, duration }: { item: ToolItem; running: boolean; duration: string | null }) {
  const args = prettyJson(item.args);
  return (
    <div className="cb">
      {args ? (
        <pre className="co" tabIndex={0}>
          {args}
        </pre>
      ) : (
        <pre className="co" tabIndex={0} data-find-skip>
          None
        </pre>
      )}
      <OutputBlock item={item} running={running} />
      <div className="cf">
        <FooterText>{duration}</FooterText>
        <InspectItem item={item} />
      </div>
    </div>
  );
}
