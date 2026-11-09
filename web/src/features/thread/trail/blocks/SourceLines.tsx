import { ChevronDown } from "lucide-react";
import { useOpenState } from "../../../../ui/openState";
import { LINES_SHOWN } from "../../toolCalls";
import { MoreBar } from "./BlockParts";

export function SourceLines({ lines, total, expandable, id }: { lines: { n: number | null; text: string }[]; total: number; expandable: boolean; id: string }) {
  const [all, setAll] = useOpenState(`src-all:${id}`, false);
  const capped = expandable && total > LINES_SHOWN && !all;
  return (
    <>
      <div className={`co ${capped ? "cap" : total > LINES_SHOWN ? "scr" : ""}`} style={{ padding: 0 }} tabIndex={total > LINES_SHOWN && !capped ? 0 : undefined}>
        <div className="src">
          {lines.map((line, at) => (
            <div key={at} className="sl">
              <i data-find-skip>{line.n ?? ""}</i>
              <span>{line.text}</span>
            </div>
          ))}
        </div>
      </div>
      {expandable && total > LINES_SHOWN ? (
        <MoreBar
          icon={<ChevronDown size={14} aria-hidden className={all ? "up" : undefined} />}
          label={all ? "Show less" : `Show all ${total} lines`}
          note={all ? `${total} lines · scrolls here` : `Fades after ${LINES_SHOWN} lines`}
          onClick={() => setAll(!all)}
        />
      ) : null}
    </>
  );
}
