import { Archive, ArchiveRestore, CircleCheck, Clock, Copy, Ellipsis, Pencil, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api } from "./api";
import type { SnoozePreset } from "../../generated/SnoozePreset";
import type { Thread } from "../../api/types";
import { useWorkspace } from "../../product/state/workspace";
import { ConfirmDialog } from "../../ui/Dialog";
import { IconButton } from "../../ui/controls";
import { Menu, type MenuItem } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";
import { archiveThreads, deleteThreadNow, settleThreads, unarchiveThread, unsettleThreads, wakeThreads } from "../workspace";
import { copyText } from "../../product/ui/copyText";
import { RenameThreadDialog, snoozeChildren } from "../workspace";

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

/** The thread's own menu at the right of its header: Settle and Snooze first, then Rename, Copy folder path, Archive (or Unarchive) and Delete thread. */
export function ThreadHeaderMenu({ thread }: { thread: Thread }) {
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [renaming, setRenaming] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const threads = useWorkspace((state) => state.threads);
  const button = useRef<HTMLDivElement>(null);
  // The same presets as the sidebar row's Snooze, read from Settings > Threads when the header mounts, so the submenu
  // is not empty the first time the menu opens.
  const [presets, setPresets] = useState<readonly SnoozePreset[]>([]);
  useEffect(() => {
    const abort = new AbortController();
    api.settings(abort.signal).then((settings) => setPresets(settings.snooze_presets ?? []), () => undefined);
    return () => abort.abort();
  }, []);
  const archived = Boolean(thread.lifecycle?.archived_at);
  const settled = Boolean(thread.lifecycle?.settled_at);
  const snoozed = Boolean(thread.lifecycle?.snoozed_until);
  const busy = familyOf(thread, threads).some((member) => member.active_run !== null);
  const icon = (Glyph: typeof Pencil) => <Glyph size={14} aria-hidden />;
  const items: MenuItem[] = [
    settled
      ? { label: "Un-settle", icon: icon(CircleCheck), onSelect: () => void unsettleThreads([thread]) }
      : { label: "Settle", icon: icon(CircleCheck), onSelect: () => void settleThreads([thread]) },
    snoozed
      ? { label: "Wake", icon: icon(Clock), onSelect: () => void wakeThreads([thread]) }
      : { label: "Snooze…", icon: icon(Clock), children: snoozeChildren([thread], presets) },
    { label: "Rename", icon: icon(Pencil), separatorBefore: true, onSelect: () => setRenaming(true) },
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
