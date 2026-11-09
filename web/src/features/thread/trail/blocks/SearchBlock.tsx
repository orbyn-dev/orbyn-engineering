import { Search, ChevronDown } from "lucide-react";
import { parsed } from "../../../../lib/timeline/toolTrace";
import type { ToolItem } from "../../../../lib/timeline/types";
import { useOpenState } from "../../../../ui/openState";
import { resultRows, ROWS_SHOWN, scalar, pluralize } from "../../toolCalls";
import { BlockHead, FooterText, MoreBar } from "./BlockParts";
import { InspectItem } from "./InspectItem";

export function SearchBlock({ item, duration }: { item: ToolItem; duration: string | null }) {
  const [all, setAll] = useOpenState(`rows-all:${item.key}`, false);
  const body = parsed(item.args);
  const rows = resultRows(item.result ?? item.output);
  const query = scalar(body.pattern ?? body.query ?? body.path);
  return (
    <div className="cb">
      <BlockHead lead={<Search size={14} />} note={`${pluralize(rows.length, "result", "results")}`}>
        {query}
      </BlockHead>
      {rows.length ? (
        <div className={`rs ${all ? (rows.length > 12 ? "scr" : "") : rows.length > ROWS_SHOWN ? "cap7" : ""}`}>
          {rows.map((row, at) => (
            <div key={at} className="r1">
              <span>{row.where}</span>
              <span>{row.text}</span>
            </div>
          ))}
        </div>
      ) : (
        <div className="co mute tool-empty">No results</div>
      )}
      {rows.length > ROWS_SHOWN ? (
        <MoreBar
          icon={<ChevronDown size={14} aria-hidden className={all ? "up" : undefined} />}
          label={all ? "Show less" : `Show all ${rows.length} results`}
          note=""
          onClick={() => setAll(!all)}
        />
      ) : null}
      <div className="cf">
        <FooterText>{duration}</FooterText>
        <InspectItem item={item} />
      </div>
    </div>
  );
}
