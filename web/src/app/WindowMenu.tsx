import { ChevronDown } from "lucide-react";
import { useRef, useState } from "react";
import { Menu, type MenuItem } from "../ui/Menu";
import { runWindowMenuItem } from "./windowControlsApi";

/** An item the shell runs: the integrated toolbar replaces the native menu bar's File, Edit, View and Window. */
const shellItem = (label: string, id: string, shortcut?: string): MenuItem => ({
  label,
  shortcut,
  onSelect: () => runWindowMenuItem(id),
});

/** An Edit item the page runs itself. */
const pageItem = (label: string, command: string): MenuItem => ({
  label,
  onSelect: () => {
    const exec = (document as Document & { execCommand?: (command: string) => boolean }).execCommand;
    if (typeof exec === "function") exec.call(document, command);
  },
});

const groups: { heading: string; items: MenuItem[] }[] = [
  {
    heading: "File",
    items: [
      shellItem("New Thread", "new-thread"),
      shellItem("Settings...", "settings", "Ctrl+,"),
      shellItem("Check for Updates...", "check-updates"),
      shellItem("Close Window", "close-window"),
    ],
  },
  {
    heading: "Edit",
    items: [
      pageItem("Undo", "undo"),
      pageItem("Redo", "redo"),
      pageItem("Cut", "cut"),
      pageItem("Copy", "copy"),
      pageItem("Paste", "paste"),
      shellItem("Paste as Text", "paste-text", "Ctrl+Shift+V"),
      pageItem("Select All", "selectAll"),
    ],
  },
  {
    heading: "View",
    items: [
      shellItem("Toggle Sidebar", "toggle-sidebar"),
      shellItem("Command Palette", "palette"),
      shellItem("Reload", "reload", "Ctrl+R"),
      shellItem("Force Reload", "force-reload", "Ctrl+Shift+R"),
      shellItem("Actual Size", "zoom-reset", "Ctrl+0"),
      shellItem("Zoom In", "zoom-in", "Ctrl+="),
      shellItem("Zoom Out", "zoom-out", "Ctrl+-"),
      shellItem("Full Screen", "fullscreen"),
    ],
  },
  {
    heading: "Window",
    items: [
      shellItem("Minimize", "minimize", "Ctrl+M"),
      shellItem("Maximize", "maximize"),
    ],
  },
];

const MENU_ITEMS: MenuItem[] = groups.flatMap(({ heading, items }, group) =>
  items.map((item, index) => ({
    ...item,
    heading: index === 0 ? heading : undefined,
    separatorBefore: index === 0 && group > 0,
  })),
);

/**
 * The Windows integrated toolbar's Menu button. It opens one popup with the File, Edit, View and
 * Window groups; the page runs Undo through Select All, and every other item goes to the shell.
 * A paste can still be refused by the web view, which has no document-level paste permission.
 */
export function WindowMenu() {
  const trigger = useRef<HTMLButtonElement>(null);
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  const [open, setOpen] = useState(false);

  const toggle = () => {
    if (open) {
      trigger.current?.focus();
      setOpen(false);
      return;
    }
    const box = trigger.current?.getBoundingClientRect();
    // Taken before the Menu mounts, so Escape and choosing an item return here even when the
    // opener was not focused by a synthetic click.
    trigger.current?.focus();
    setAt({ x: box?.left ?? 8, y: box ? box.bottom + 4 : 8 });
    setOpen(true);
  };

  return (
    <>
      <button
        ref={trigger}
        type="button"
        className="window-menu-trigger"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={toggle}
      >
        Menu
        <ChevronDown size={13} aria-hidden />
      </button>
      {open && at ? (
        <Menu
          x={at.x}
          y={at.y}
          origin="top left"
          label="Menu"
          minWidth={236}
          items={MENU_ITEMS}
          onClose={() => setOpen(false)}
        />
      ) : null}
    </>
  );
}
