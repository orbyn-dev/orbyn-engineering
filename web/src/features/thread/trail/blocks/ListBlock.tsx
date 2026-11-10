import { FileText, Folder, FolderOpen, ChevronDown } from "lucide-react";
import type { ToolItem } from "../../../../product/timeline/types";
import { useOpenState } from "../../../../ui/openState";
import { ENTRIES_SHOWN, pluralize } from "../../toolCalls";
import { BlockHead, Dot, FooterText, MoreBar } from "./BlockParts";
import { InspectItem } from "./InspectItem";

export function ListBlock({ item, entries, duration, onOpen, shown }: { item: ToolItem; entries: string[]; duration: string | null; onOpen: (() => void) | null; shown: string | null }) {
  const [all, setAll] = useOpenState(`rows-all:${item.key}`, false);
  return (
    <div className="cb">
      <BlockHead lead={<Folder size={14} />}>{shown ?? "Folder"}</BlockHead>
      {entries.length ? (
        <div className={`rs ${all ? "" : entries.length > ENTRIES_SHOWN ? "cap6" : ""}`}>
          {entries.map((name, at) => (
            <div key={at} className="r1 one">
              {name.endsWith("/") ? <Folder size={14} aria-hidden /> : <FileText size={14} aria-hidden />}
              <span>{name}</span>
            </div>
          ))}
        </div>
      ) : (
        <div className="co mute tool-empty">Nothing here</div>
      )}
      {entries.length > ENTRIES_SHOWN ? (
        <MoreBar
          icon={<ChevronDown size={14} aria-hidden className={all ? "up" : undefined} />}
          label={all ? "Show less" : `Show all ${entries.length} entries`}
          note=""
          onClick={() => setAll(!all)}
        />
      ) : null}
      <div className="cf">
        <FooterText>
          {pluralize(entries.length, "entry", "entries")}
          {duration ? <> <Dot />{duration}</> : null}
        </FooterText>
        {onOpen ? (
          <button type="button" className="bt" onClick={onOpen}>
            <FolderOpen size={14} aria-hidden />
            <span>Open in Files</span>
          </button>
        ) : null}
        <InspectItem item={item} />
      </div>
    </div>
  );
}
