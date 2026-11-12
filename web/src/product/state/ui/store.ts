import { create } from "zustand";
import { useWorkspace } from "../workspace";
import { readJson, writeJson } from "../../../lib/storage";
import {
  DRAFT_LAYOUT,
  RIGHT_TAB_IDS,
  defaultOrder,
  isFileTab,
  withTab,
  type ContextTab,
  type PaletteStart,
  type Prefs,
  type RightTab,
  type ThemeChoice,
  type ThreadNavigationOrigin,
  type ToastAction,
  type ToastKind,
} from "./types";
import { fileTabId, type FileTarget } from "./fileTab";
import { PREFS_KEY, currentLayout, defaults, layoutDefaults, withLayout } from "./defaults";
import { foldersMigrated, prefsOf, savedPrefs } from "./savedPrefs";

const TOAST_MS = 4000;
let toastCounter = 0;
const DRAFTS_KEY = "orbyn.drafts.v1";

interface UiState extends Prefs {
  /** Counts saved server preferences, so a mounted reader can read them again. Never persisted. */
  preferencesRevision: number;
  bumpPreferencesRevision(): void;
  terminalReveal: { threadId: string; group: string; nonce: number } | null;
  revealTerminal(threadId: string, group: string): void;
  drafts: Record<string, string>;
  palette: PaletteStart | null;
  setPalette(palette: PaletteStart | null): void;
  lastThreadId: string | null;
  toasts: { id: number; text: string; kind?: ToastKind; action?: ToastAction }[];
  pushToast(text: string, kind?: ToastKind, action?: ToastAction): void;
  dismissToast(id: number): void;
  /** The open welcome step, or null; not saved, so it lasts while the app runs. */
  welcomeStep: number | null;
  setWelcomeStep(step: number | null): void;
  /** True once, when the migration turned project folders on; not saved. */
  foldersNotice: boolean;
  /** Closes the notice; with `undo`, also turns project folders back off. */
  closeFoldersNotice(undo: boolean): void;
  setTheme(theme: ThemeChoice): void;
  /** Opens `tab`, or closes its surface if `tab` is already showing. */
  /** A file the Files surface should open next, with the line to reveal. */
  fileReveal: ThreadNavigationOrigin & { path: string; line: number | null; nonce: number } | null;
  revealFile(path: string, line: number | null, origin: ThreadNavigationOrigin): void;
  consumeFileReveal(nonce: number): void;
  /** A message to scroll to when its thread is showing. */
  messageReveal: { threadId: string; messageId: string } | null;
  revealMessage(threadId: string, messageId: string): void;
  clearMessageReveal(): void;
  /** A file the Diff surface should scroll to. */
  diffReveal: ThreadNavigationOrigin & { path: string; nonce: number } | null;
  /** A commit the Diff surface should show next. */
  commitReveal: { sha: string; nonce: number } | null;
  revealCommit(sha: string): void;
  revealDiff(path: string, origin: ThreadNavigationOrigin): void;
  consumeDiffReveal(nonce: number): void;
  togglePanel(tab: ContextTab): void;
  /** Takes a surface out of this thread's tab row; closing the open one opens its neighbour, and the last one leaves the panel empty. */
  closePanelTab(tab: RightTab): void;
  /** Closes every tab but this one, which opens. */
  closeOtherPanelTabs(tab: RightTab): void;
  /** Closes every tab: the panel stays open, empty. */
  closeAllPanelTabs(): void;
  /** Puts a closed surface back in the row, opens it and shows the panel. */
  reopenPanelTab(tab: RightTab): void;
  /** Opens a file from the left bar as a tab, or shows the one already open. */
  openFileTab(target: FileTarget): void;
  /** Moves a tab to `index` in the row's order. */
  movePanelTab(tab: RightTab, index: number): void;
  /** The default order. Which tabs are open stays as it is. */
  resetPanelTabs(): void;
  /** A page floating over the chat, if any. */
  previewFloat: { threadId: string; tabId: string } | null;
  /** A URL to open, or an existing tab to reveal in the Browser panel. */
  previewRequest: string | { tabId: string } | null;
  setPreviewFloat(float: { threadId: string; tabId: string } | null): void;
  showInBrowser(url: string | { tabId: string } | null): void;
  closePanel(): void;
  setPanelOpen(open: boolean): void;
  /** On a phone-width window the panel starts closed and what happens there is not remembered. */
  setPanelNarrow(narrow: boolean): void;
  /** The remembered choice, held aside while a phone-width window uses its own. */
  desktopPanelOpen: boolean | null;
  setPanelAnimationMs(ms: number): void;
  setPanelMaximized(maximized: boolean): void;
  setPanelWidth(width: number | null): void;
  /** Shows another thread's layout, restoring what it last had. */
  setLayoutKey(key: string): void;
  /** A sent draft keeps its layout as a thread, and the next draft starts fresh. */
  adoptDraftLayout(threadId: string): void;
  toggleSidebar(): void;
  setSidebarWidth(width: number): void;
  toggleGroup(group: string, fallback: boolean): void;
  setPrefs(
    prefs: Partial<Omit<Prefs, "layouts" | "layoutKey">>,
  ): void;
  setDraft(threadId: string, text: string): void;
  setLastThread(threadId: string): void;
}

export const useUi = create<UiState>((set, get) => {
  let navigationNonce = 0;
  const savePrefs = () => {
    const state = get();
    const layouts = withLayout(state.layouts, state.layoutKey, currentLayout(state));
    set({ layouts });
    writeJson(PREFS_KEY, prefsOf({ ...state, layouts }));
  };
  return {
    ...defaults,
    ...savedPrefs(),
    foldersNotice: foldersMigrated,
    closeFoldersNotice(undo) {
      set({ foldersNotice: false });
      if (undo) get().setPrefs({ projectFolders: false });
    },
    welcomeStep: null,
    setWelcomeStep(welcomeStep) {
      set({ welcomeStep });
    },
    terminalReveal: null,
    revealTerminal(threadId, group) {
      set((state) => ({ terminalOpen: true, terminalReveal: { threadId, group, nonce: (state.terminalReveal?.nonce ?? 0) + 1 } }));
    },
    drafts: readJson<Record<string, string>>(DRAFTS_KEY) ?? {},
    palette: null,
    setPalette(palette) {
      set({ palette });
    },
    lastThreadId: null,
    toasts: [],
    pushToast(text, kind, action) {
      const id = ++toastCounter;
      set((state) => ({ toasts: [...state.toasts, { id, text, kind, ...(action ? { action } : {}) }] }));
      setTimeout(() => get().dismissToast(id), TOAST_MS);
    },
    dismissToast(id) {
      set((state) => ({ toasts: state.toasts.filter((toast) => toast.id !== id) }));
    },
    setTheme(theme) {
      set({ theme });
      savePrefs();
    },
    diffReveal: null,
    commitReveal: null,
    revealCommit(sha) {
      set({
        commitReveal: { sha, nonce: Date.now() },
        panelOpen: true,
        panelTab: "diff",
        panelTabsClosed: get().panelTabsClosed.filter((id) => id !== "diff"),
      });
    },
    revealDiff(path, origin) {
      set({
        diffReveal: { ...origin, path, nonce: ++navigationNonce },
        fileReveal: null,
        panelOpen: true,
        panelTab: "diff",
        panelTabsClosed: get().panelTabsClosed.filter((id) => id !== "diff"),
      });
    },
    consumeDiffReveal(nonce) {
      set((state) => state.diffReveal?.nonce === nonce ? { diffReveal: null } : state);
    },
    fileReveal: null,
    messageReveal: null,
    revealMessage(threadId, messageId) {
      set({ messageReveal: { threadId, messageId } });
    },
    clearMessageReveal() {
      set({ messageReveal: null });
    },
    revealFile(path, line, origin) {
      const id = fileTabId({ kind: "thread", path });
      set({
        fileReveal: { ...origin, path, line, nonce: ++navigationNonce },
        diffReveal: null,
        panelOpen: true,
        panelTab: id,
        panelTabOrder: withTab(get().panelTabOrder, id),
      });
      savePrefs();
    },
    openFileTab(target) {
      const id = fileTabId(target);
      set({ panelOpen: true, panelTab: id, panelTabOrder: withTab(get().panelTabOrder, id) });
      savePrefs();
    },
    consumeFileReveal(nonce) {
      set((state) => state.fileReveal?.nonce === nonce ? { fileReveal: null } : state);
    },
    previewFloat: null,
    previewRequest: null,
    setPreviewFloat(previewFloat) {
      set({ previewFloat });
    },
    showInBrowser(url) {
      if (url === null) {
        set({ previewRequest: null });
        return;
      }
      set({
        previewRequest: url,
        panelOpen: true,
        panelTab: "browser",
        panelTabsClosed: get().panelTabsClosed.filter((id) => id !== "browser"),
      });
    },
    togglePanel(tab) {
      if (tab === "terminal") {
        set((state) => ({ terminalOpen: !state.terminalOpen }));
      } else {
        const { panelOpen, panelTab } = get();
        set(
          panelOpen === true && panelTab === tab
            ? { panelOpen: false }
            : {
                panelOpen: true,
                panelTab: tab,
                panelTabsClosed: get().panelTabsClosed.filter((id) => id !== tab),
              },
        );
      }
      savePrefs();
    },
    closePanelTab(tab) {
      const { panelTabOrder, panelTabsClosed, panelTab } = get();
      const row = panelTabOrder.filter((id) => isFileTab(id) || !panelTabsClosed.includes(id));
      if (!row.includes(tab)) return;
      const at = row.indexOf(tab);
      const shown = panelTab && row.includes(panelTab) ? panelTab : row[0];
      set({
        // A surface of the panel's own stays in the order, closed; a file tab is simply gone.
        ...(isFileTab(tab)
          ? { panelTabOrder: panelTabOrder.filter((id) => id !== tab) }
          : { panelTabsClosed: [...panelTabsClosed, tab] }),
        ...(shown === tab ? { panelTab: row[at + 1] ?? row[at - 1] ?? null } : {}),
      });
      savePrefs();
    },
    closeOtherPanelTabs(tab) {
      const { panelTabOrder, panelTabsClosed } = get();
      if (!isFileTab(tab) && panelTabsClosed.includes(tab)) return;
      set({
        panelTabsClosed: RIGHT_TAB_IDS.filter((id) => id !== tab),
        panelTabOrder: panelTabOrder.filter((id) => !isFileTab(id) || id === tab),
        panelTab: tab,
      });
      savePrefs();
    },
    closeAllPanelTabs() {
      set({ panelTabsClosed: [...RIGHT_TAB_IDS], panelTabOrder: get().panelTabOrder.filter((id) => !isFileTab(id)), panelTab: null });
      savePrefs();
    },
    reopenPanelTab(tab) {
      set({
        panelTabsClosed: get().panelTabsClosed.filter((id) => id !== tab),
        panelTabOrder: withTab(get().panelTabOrder, tab),
        panelOpen: true,
        panelTab: tab,
      });
      savePrefs();
    },
    movePanelTab(tab, index) {
      const order = get().panelTabOrder.filter((id) => id !== tab);
      order.splice(Math.max(0, Math.min(index, order.length)), 0, tab);
      set({ panelTabOrder: order });
      savePrefs();
    },
    resetPanelTabs() {
      set({ panelTabOrder: defaultOrder(get().panelTabOrder) });
      savePrefs();
    },
    closePanel() {
      set({ panelOpen: false });
      savePrefs();
    },
    desktopPanelOpen: null,
    setPanelNarrow(narrow) {
      const { desktopPanelOpen, panelOpen } = get();
      if (narrow && desktopPanelOpen === null) {
        set({ desktopPanelOpen: panelOpen, panelOpen: false });
      } else if (!narrow && desktopPanelOpen !== null) {
        set({ panelOpen: desktopPanelOpen, desktopPanelOpen: null });
      }
    },
    setPanelOpen(open) {
      set({ panelOpen: open });
      savePrefs();
    },
    setPanelAnimationMs(ms) {
      set({ panelAnimationMs: Math.min(Math.max(ms, 0), 400) });
      savePrefs();
    },
    setPanelMaximized(maximized) {
      set({ panelMaximized: maximized });
      savePrefs();
    },
    setPanelWidth(width) {
      set({ panelWidth: width });
      savePrefs();
    },
    setLayoutKey(key) {
      const state = get();
      if (key === state.layoutKey) return;
      const layouts = withLayout(state.layouts, state.layoutKey, currentLayout(state));
      set({ layouts, layoutKey: key, ...(layouts[key] ?? layoutDefaults) });
      savePrefs();
    },
    adoptDraftLayout(threadId) {
      const state = get();
      if (state.layoutKey !== DRAFT_LAYOUT) return;
      const { [DRAFT_LAYOUT]: _draft, ...others } = state.layouts;
      set({ layouts: others, layoutKey: threadId });
      savePrefs();
    },
    preferencesRevision: 0,
    bumpPreferencesRevision() {
      set((state) => ({ preferencesRevision: state.preferencesRevision + 1 }));
    },
    toggleSidebar() {
      set((state) => ({ sidebarCollapsed: !state.sidebarCollapsed }));
      savePrefs();
    },
    setSidebarWidth(width) {
      set({ sidebarWidth: width });
      savePrefs();
    },
    toggleGroup(group, fallback) {
      set((state) => ({ groups: { ...state.groups, [group]: !(state.groups[group] ?? fallback) } }));
      savePrefs();
    },
    setPrefs(prefs) {
      set(prefs);
      savePrefs();
    },
    setDraft(threadId, text) {
      set((state) => {
        const drafts = { ...state.drafts };
        if (text) drafts[threadId] = text;
        else delete drafts[threadId];
        return { drafts };
      });
      writeJson(DRAFTS_KEY, get().drafts);
    },
    setLastThread(threadId) {
      set({ lastThreadId: threadId });
    },
  };
});

// Navigation cancellation must outlive a closed/unmounted surface. These intents
// are transient and are never written by savePrefs.
const cancelPathNavigation = () => useUi.setState({ fileReveal: null, diffReveal: null });
window.addEventListener("hashchange", cancelPathNavigation);
const unsubscribePathNavigation = useWorkspace.subscribe((state) => {
  const { fileReveal, diffReveal, consumeFileReveal, consumeDiffReveal } = useUi.getState();
  if (fileReveal && state.threads[fileReveal.threadId]?.cwd !== fileReveal.cwd) consumeFileReveal(fileReveal.nonce);
  if (diffReveal && state.threads[diffReveal.threadId]?.cwd !== diffReveal.cwd) consumeDiffReveal(diffReveal.nonce);
});
import.meta.hot?.dispose(() => {
  window.removeEventListener("hashchange", cancelPathNavigation);
  unsubscribePathNavigation();
});
