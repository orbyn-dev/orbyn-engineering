import { Archive, ArchiveRestore, Copy, Ellipsis, Pencil, Trash2 } from "lucide-react";
import { useRef, useState } from "react";
import type { Thread } from "../../api/types";
import { useWorkspace } from "../../state/workspace";
import { ConfirmDialog } from "../../ui/Dialog";
import { IconButton } from "../../ui/controls";
import { Menu, type MenuItem } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";
import { archiveThreads, copyText, deleteThreadNow, unarchiveThread } from "../workspace/threadActions";
import { RenameThreadDialog } from "../workspace/threadMenu";

/** Every thread below this one (side chats, children) and the thread itself. */
export function familyOf(thread: Thread, threads: Readonly<Record<string, Thread>>): Thread[] {
  const family = [thread];
  for (let at = 0; at < family.length; at++) {
    const parent = family[at]!;
    for (const other of Object.values(threads)) {
      if (other.lifecycle?.parent_thread_id === parent.id && !family.includes(other)) family.push(other);
    }
  }
  return family;
}

const MENU_WIDTH = 220;

/** The thread's own menu at the right of its header: Rename, Copy folder path, Archive (or Unarchive) and Delete thread. */
export function ThreadHeaderMenu({ thread }: { thread: Thread }) {
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [renaming, setRenaming] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const threads = useWorkspace((state) => state.threads);
  const button = useRef<HTMLDivElement>(null);
  const archived = Boolean(thread.lifecycle?.archived_at);
  const busy = familyOf(thread, threads).some((member) => member.active_run !== null);
  const icon = (Glyph: typeof Pencil) => <Glyph size={14} aria-hidden />;
  const items: MenuItem[] = [
    { label: "Rename", icon: icon(Pencil), onSelect: () => setRenaming(true) },
    { label: "Copy folder path", icon: icon(Copy), onSelect: () => void copyText(thread.cwd, "Folder path copied") },
    archived
      ? {
          label: "Unarchive",
          icon: icon(ArchiveRestore),
          disabled: thread.project_removed === true,
          title: thread.project_removed ? "Add the project folder back to unarchive this thread" : undefined,
          onSelect: () => void unarchiveThread(thread),
        }
      : { label: "Archive", icon: icon(Archive), disabled: thread.active_run !== null, onSelect: () => void archiveThreads([thread]) },
    {
      label: "Delete thread",
      icon: icon(Trash2),
      destructive: true,
      disabled: busy,
      title: busy ? "Stop the running work in this thread, its side chats and its children first" : undefined,
      separatorBefore: true,
      onSelect: () => setDeleting(true),
    },
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
          // Kept inside the window, whatever the layout puts the button beside.
          const x = Math.min(Math.max(rect.right - MENU_WIDTH, 8), window.innerWidth - MENU_WIDTH - 8);
          setMenu(menu ? null : { x, y: rect.bottom + 4 });
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
          onConfirm={() => deleteThreadNow(thread)}
        >
          <p>
            Its messages and checkpoints are deleted, and so are its side chats and any threads that branch from it. Code changes in its
            worktree stay.
          </p>
        </ConfirmDialog>
      ) : null}
    </div>
  );
}
