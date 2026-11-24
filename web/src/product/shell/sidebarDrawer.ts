import { create } from "zustand";
import { navigate } from "../state/route";
import { useUi } from "../state/ui";

/**
 * On small windows (600 to 899 px) the sidebar is a drawer over the thread, opened from the title
 * bar's sidebar button. It starts closed and is not saved.
 */
export const useSidebarDrawer = create<{ open: boolean; setOpen(open: boolean): void; toggle(): void; close(): void }>((set) => ({
  open: false,
  setOpen: (open) => set({ open }),
  toggle: () => set((state) => ({ open: !state.open })),
  close: () => set({ open: false }),
}));

/**
 * The title bar button and the sidebar shortcut: opening the drawer closes the side panel, as a small window fits only one.
 */
export function toggleSidebarDrawer() {
  const opening = !useSidebarDrawer.getState().open;
  if (opening) useUi.getState().closePanel();
  useSidebarDrawer.getState().setOpen(opening);
}

/**
 * The phone's sidebar, which is a full-screen sheet over any page that is not home and the page itself
 * at home. The title bar button and the sidebar shortcut both come here, so the key and the control it
 * stands for cannot drift apart: close the sheet, leave for the composer when the sidebar already is the
 * page, or open the sheet over the page.
 */
export function togglePhoneSidebar(sidebarIsPage: boolean) {
  const drawer = useSidebarDrawer.getState();
  if (drawer.open) drawer.close();
  else if (sidebarIsPage) navigate({ name: "new", projectId: null });
  else toggleSidebarDrawer();
}
