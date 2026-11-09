import { FileDiff as FileDiffIcon, FileText, FolderOpen, SquarePen } from "lucide-react";
import { editDiff, readRange, sourceLines, pluralize, editStat } from "../../toolCalls";
import { BlockHead } from "./BlockParts";
import { GenericBlock } from "./GenericBlock";
import { InspectItem } from "./InspectItem";
import { SourceLines } from "./SourceLines";
import type { BlockContext } from "./types";

/** A file read: its lines, with a way to open it. */
export function readBlock({ item, origin, path, shown, revealFile, timing }: BlockContext) {
  const range = readRange(item);
  const lines = sourceLines(item.result, range?.from ?? 1);
  return (
    <div className="cb">
      <BlockHead lead={<FileText size={14} />} note={range ? `lines ${range.from}${range.to !== null ? `–${range.to}` : ""}` : null}>
        {shown ?? "File"}
      </BlockHead>
      <SourceLines lines={lines} total={lines.length} expandable id={item.key} />
      <div className="cf">
        {timing(lines.length ? pluralize(lines.length, "line", "lines") : null)}
        {path ? (
          <button type="button" className="bt" onClick={() => revealFile(path, range?.from ?? null, origin)}>
            <FolderOpen size={14} aria-hidden />
            <span>Open file</span>
          </button>
        ) : null}
        <InspectItem item={item} />
      </div>
    </div>
  );
}

/** A file edit as a small diff. */
export function editBlock({ item, running, origin, duration, path, shown, revealDiff, timing }: BlockContext) {
  const diff = editDiff(item);
  const stat = editStat(item);
  if (!diff) return <GenericBlock item={item} running={running} duration={duration} />;
  return (
    <div className="cb">
      <BlockHead lead={<SquarePen size={14} />}>{shown ?? "Files"}</BlockHead>
      <div className="hunk">
        {diff.removed.map((line, at) => (
          <div key={`d${at}`} className="del">
            <span data-find-skip />
            <span data-find-skip>−</span>
            <span>{line}</span>
          </div>
        ))}
        {diff.added.map((line, at) => (
          <div key={`a${at}`} className="add">
            <span data-find-skip />
            <span data-find-skip>+</span>
            <span>{line}</span>
          </div>
        ))}
      </div>
      <div className="cf">
        {timing(stat ? `+${stat.added} −${stat.removed}` : null)}
        {path ? (
          <button type="button" className="bt" onClick={() => revealDiff(path, origin)}>
            <FileDiffIcon size={14} aria-hidden />
            <span>Open in Diff</span>
          </button>
        ) : null}
        <InspectItem item={item} />
      </div>
    </div>
  );
}

/** A new file's content. */
export function createBlock({ item, running, origin, body, duration, path, shown, revealFile, timing }: BlockContext) {
  const content = typeof body.content === "string" ? body.content : null;
  if (content === null) return <GenericBlock item={item} running={running} duration={duration} />;
  const lines = content.replace(/\n$/, "").split("\n").map((text, at) => ({ n: at + 1, text }));
  return (
    <div className="cb">
      <BlockHead lead={<FileText size={14} />}>{shown ?? "File"}</BlockHead>
      <SourceLines lines={lines} total={lines.length} expandable id={item.key} />
      <div className="cf">
        {timing(`${/created successfully/i.test(item.result ?? "") ? "New file" : "File"} · ${pluralize(lines.length, "line", "lines")}`)}
        {path ? (
          <button type="button" className="bt" onClick={() => revealFile(path, null, origin)}>
            <FolderOpen size={14} aria-hidden />
            <span>Open file</span>
          </button>
        ) : null}
        <InspectItem item={item} />
      </div>
    </div>
  );
}
