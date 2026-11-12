import { ChevronRight, Folder, Hash, Search } from "lucide-react";
import type { ReactNode } from "react";
import { useWaited } from "../../../lib/motion";
import { IconButton } from "../../../ui/controls";

/** The folders and name of a file, with Find and Go to line (off, for a file that has no text to search). */
function EditorCrumb({ path, disabled }: { path: string; disabled?: boolean }) {
  const folders = path.split("/").slice(0, -1);
  const name = path.split("/").at(-1) ?? path;
  return (
    <div className="ed-crumb">
      <Folder size={14} aria-hidden />
      {folders.map((part, index) => (
        <span key={index} className="ed-part">
          {part}
          <ChevronRight size={12} aria-hidden />
        </span>
      ))}
      <b title={path}>{name}</b>
      <span className="ed-sp" />
      <IconButton label="Find ⌘F" disabled={disabled}>
        <Search size={14} aria-hidden />
      </IconButton>
      <IconButton label="Go to line ⌘G" disabled={disabled}>
        <Hash size={14} aria-hidden />
      </IconButton>
    </div>
  );
}

const WIDTHS = [62, 48, 0, 71, 55, 66, 40, 0, 58, 44];

/** editor-loading: the tab, the name and the folders are there; grey lines stand where the code will be once the wait passes 400 ms. */
export function EditorLoading({ path }: { path: string }) {
  const slow = useWaited(true);
  return (
    <div className="ed" aria-busy="true">
      <EditorCrumb path={path} />
      <div className="ed-skel" role="status">
        <span className="sr-only">Loading file</span>
        {slow ? WIDTHS.map((width, at) => <i key={at} aria-hidden style={{ width: `${width}%`, opacity: width === 0 ? 0 : 1 }} />) : null}
      </div>
      <div className="ed-status">
        <span />
        <span />
        <span className="ed-end">Opening…</span>
      </div>
    </div>
  );
}

/** editor-binary: a file with no text. */
export function EditorEmpty({ path, icon, title, children, actions }: { path: string; icon: ReactNode; title: string; children: ReactNode; actions?: ReactNode }) {
  return (
    <div className="ed">
      <EditorCrumb path={path} disabled />
      <div className="ed-empty">
        {icon}
        <h4>{title}</h4>
        <span>{children}</span>
        {actions ? <div className="ed-chips">{actions}</div> : null}
      </div>
    </div>
  );
}
