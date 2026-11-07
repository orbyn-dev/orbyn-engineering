import { Archive, Copy, Ellipsis, Pencil, Trash2 } from "lucide-react";
import { useRef, useState } from "react";
import type { Thread } from "../../api/types";
import { ConfirmDialog } from "../../ui/Dialog";
import { IconButton } from "../../ui/controls";
import { Menu, type MenuItem } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";
import { archiveThreads, copyText, deleteThreads } from "../workspace/threadActions";
import { RenameThreadDialog } from "../workspace/threadMenu";

/** The thread's own menu at the right of its header: Rename, Copy folder path, Archive and Delete thread. */
export function ThreadHeaderMenu({ thread }: { thread: Thread }) {
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [renaming, setRenaming] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const button = useRef<HTMLDivElement>(null);
  const running = thread.active_run !== null;
  const icon = (Glyph: typeof Pencil) => <Glyph size={14} aria-hidden />;
  const items: MenuItem[] = [
    { label: "Rename", icon: icon(Pencil), onSelect: () => setRenaming(true) },
    { label: "Copy folder path", icon: icon(Copy), onSelect: () => void copyText(thread.cwd, "Folder path copied") },
    { label: "Archive", icon: icon(Archive), disabled: running, onSelect: () => void archiveThreads([thread]) },
    { label: "Delete thread", icon: icon(Trash2), destructive: true, disabled: running, separatorBefore: true, onSelect: () => setDeleting(true) },
  ];
  return (
    <div ref={button} className="menu-anchor">
      <IconButton
        label="Thread menu"
        aria-haspopup="menu"
        aria-expanded={menu !== null}
        pressed={menu !== null}
        onClick={(event) => {
          const rect = event.currentTarget.getBoundingClientRect();
          setMenu(menu ? null : { x: rect.right - 200, y: rect.bottom + 4 });
        }}
      >
        <Ellipsis size={16} aria-hidden />
      </IconButton>
      <Presence>{menu ? <Menu {...menu} label="Thread menu" items={items} onClose={() => setMenu(null)} /> : null}</Presence>
      {renaming ? <RenameThreadDialog thread={thread} onClose={() => setRenaming(false)} /> : null}
      {deleting ? (
        <ConfirmDialog
          title={`Delete “${thread.title}”?`}
          confirmLabel="Delete thread"
          tone="danger"
          onClose={() => setDeleting(false)}
          onConfirm={() => deleteThreads([thread])}
        >
          <p>Its messages and checkpoints are deleted. Code changes in its worktree stay.</p>
        </ConfirmDialog>
      ) : null}
    </div>
  );
}
