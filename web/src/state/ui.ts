import { create } from "zustand";
import { presetById, THEME_TOKENS, type ThemeColors } from "../styles/themes";
import type { ProjectGroupingMode } from "../generated/ProjectGroupingMode";
import { MOTION } from "../lib/motion";
import { readJson, writeJson } from "../lib/storage";
import { useWorkspace } from "./workspace";

export type ThemeChoice = "system" | "light" | "dark";
/** How the workspace is arranged: by project, by what needs you, or as one reading column. */
/** What a toast leads with: a thread needs you, finished or failed, or a copy was made. Others are text only. */
export type ToastKind = "needs" | "done" | "failed" | "copied";
/** A button on a toast, such as View. Using it closes the toast. */
export interface ToastAction { label: string; run(): void }

export type LayoutChoice = "console" | "ledger" | "reading";
export const LAYOUT_CHOICES: readonly LayoutChoice[] = ["console", "ledger", "reading"];
export type RightTab = "diff" | "files" | "activity" | "browser" | "side" | "tools";
/** The panel's surfaces in their default order. Terminal is the header button and the drawer, never a tab. */
export const RIGHT_TAB_IDS: readonly RightTab[] = ["diff", "files", "browser", "activity", "side", "tools"];
/** Tabs that are closed until opened from their own entry point, such as the details card's Tools row. */
export const CLOSED_AT_FIRST: readonly RightTab[] = ["tools"];

/** A saved order or closed set may be stale: unknown and repeated ids go, missing ones return at the end, and one tab always stays. */
export function cleanTabs(order: unknown, closed: unknown): { order: RightTab[]; closed: RightTab[] } {
  const valid = (list: unknown): RightTab[] =>
    (Array.isArray(list) ? list : []).filter((id, at, all): id is RightTab => RIGHT_TAB_IDS.includes(id as RightTab) && all.indexOf(id) === at);
  const first = valid(order);
  const shut = valid(closed);
  // A saved row from before a tab existed has not seen it, so such a tab starts closed.
  for (const id of CLOSED_AT_FIRST) if (!first.includes(id) && !shut.includes(id)) shut.push(id);
  return { order: [...first, ...RIGHT_TAB_IDS.filter((id) => !first.includes(id))], closed: shut.length >= RIGHT_TAB_IDS.length ? [] : shut };
}
export type ContextTab = RightTab | "terminal";
export interface ThreadNavigationOrigin { threadId: string; cwd: string }

interface Prefs {
  theme: ThemeChoice;
  /** The surface pane, one choice for every thread. It opens only when asked, or for
   * something the person triggered (a diff, file or pull request chip), at any width. */
  panelOpen: boolean;
  /** The surface showing; null when none is. */
  panelTab: RightTab | null;
  /** The tab row's order for this thread. */
  panelTabOrder: RightTab[];
  /** Surfaces closed in this thread's tab row; the "+" brings them back. */
  panelTabsClosed: RightTab[];
  /** Panel and hero motion in ms, 0 to 400. 0 is off; the default is the --m-slow token. */
  panelAnimationMs: number;
  /** The floating details card on wide layouts. */
  detailsOpen: boolean;
  /** The terminal drawer under the conversation. */
  terminalOpen: boolean;
  panelMaximized: boolean;
  /** The pane's width for this thread; null follows the window. */
  panelWidth: number | null;
  /** The side chat the panel shows. */
  sideChat: string | null;
  /** The current thread's layout is the fields above; others wait here. */
  layouts: Record<string, ThreadLayout>;
  layoutKey: string;
  sidebarCollapsed: boolean;
  sidebarWidth: number;
  /** Sidebar groups the viewer has opened or closed. */
  groups: Record<string, boolean>;
  /** Projects the thread list is limited to; empty is all. */
  projectFilter: string[];
  /** Folders the active list by project. */
  projectFolders: boolean;
  /** How folders combine registered checkouts; separate keeps one per project. */
  projectGrouping: ProjectGroupingMode;
  /** A grouping recipe override for one physical project, by project ID. */
  projectGroupingOverrides: Record<string, ProjectGroupingMode>;
  /** Lets folder rows wrap their title. */
  multilineRows: boolean;
  threadSort: "updated_at" | "created_at";
  projectSort: "updated_at" | "created_at";
  /** `engine:model` keys. */
  modelHidden: string[];
  modelOrder: string[];
  effortDisplay: "label" | "bars";
  preferredEditor: string | null;
  /** TeX in rendered Markdown, off unless asked for. */
  mathRendering: boolean;
  chatWidth: "comfortable" | "wide" | "full";
  workspaceLayout: LayoutChoice;
  /** Wraps long lines in code and diffs. */
  wordWrap: boolean;
  timestampFormat: "locale" | "12-hour" | "24-hour";
  sendShortcut: "enter" | "mod-enter-multiline" | "mod-enter";
  followUpBehavior: "queue" | "steer";
  diffLayout: "stacked" | "split";
  previewSearchEngine: string;
  /** Opens Changes after a large turn. */
  proactivePanels: boolean;
  recentUrls: string[];
  /** Where a link in a thread opens. */
  linkTarget: "system" | "in-app";
  /** Floats the page when an agent opens one and the Browser panel isn't showing. */
  autoFloatPreview: boolean;
  diffFilesCollapsed: boolean;
  diffIgnoreWhitespace: boolean;
  diffHideTests: boolean;
  /** A named palette over the base theme, for each scheme. */
  lightPreset: string | null;
  darkPreset: string | null;
  /** Colours the person set themselves, which win over any preset. */
  customColors: ThemeColors;
  typography: Typography;
  contrast: "normal" | "more";
  fontSmoothing: boolean;
  /** Keeps checkout and branch under the composer after the first send. */
  persistentComposerContext: boolean;
  /** Lists skills in the `/` menu as well as under `$`. */
  skillsInSlashMenu: boolean;
  /** Offers Plan with its button, commands and Shift+Tab. */
  planControls: boolean;
  /** The circle in the composer showing how full the context is. */
  contextMeter: boolean;
  /** Saved prompts by thread, newest first. */
  stashes: Record<string, StashedPrompt[]>;
  threadNotifications: boolean;
  /** A short tone for the same events, with or without the system alert. */
  threadSound: boolean;
  inAppNotifications: boolean;
  /** Tells you when a scheduled task's run finishes or fails. The service keeps no such setting, so it lives here. */
  taskRunNotifications: boolean;
  confirmUnpin: boolean;
  confirmArchive: boolean;
  confirmDelete: boolean;
  /** The last choice, which new threads start from. */
  lastModel: { engine: string; model: string | null; effort: string | null } | null;
  /** The project the unsent draft belongs to, so its row opens it there. */
  draftProject: string | null;
  /** Pull request links open in the panel instead of on the host. */
  pullLinksInApp: boolean;
  /** Thread details rows turned off. */
  detailsHidden: string[];
  /** Moves working threads, and ones waiting on delegated work, to their own shelf. */
  workingSection: boolean;
  /** Provider updates already dismissed, as `engine:version`. */
  /** The environment theme's `set_at` this viewer last applied; 0 is never. */
  environmentThemeAppliedAt: number;
  /** How the sidebar marks this environment's build: channel artwork, a version pill or nothing. */
  environmentIdentification: EnvironmentIdentification;
  /** The welcome steps were finished or skipped on this device. */
  welcomeDone: boolean;
}

export type EnvironmentIdentification = "artwork" | "pill" | "none";

export interface Typography {
  uiFont: string;
  /** Interface size in px; everything scales from it. */
  uiSize: number;
  chatSize: number;
  codeFont: string;
  codeSize: number;
  terminalSize: number;
}

export interface StashedPrompt {
  id: string;
  text: string;
  at: string;
}

/** What each thread remembers of its layout. Drafts share the `draft` key until sent. */
export interface ThreadLayout {
  panelTab: RightTab | null;
  panelTabOrder: RightTab[];
  panelTabsClosed: RightTab[];
  detailsOpen: boolean;
  terminalOpen: boolean;
  panelMaximized: boolean;
  panelWidth: number | null;
  sideChat: string | null;
}

export const DRAFT_LAYOUT = "draft";
const MAX_LAYOUTS = 200;

export type PaletteStart = "root" | "add-project" | "add-project-url" | "add-project-new" | "files" | "search";

export const DEFAULT_SIDEBAR_WIDTH = 224;
const TOAST_MS = 4000;
let toastCounter = 0;

const PREFS_KEY = "orbyn.prefs.v1";
/** Own key, so a stale tab from an older build that rewrites the prefs cannot undo the migration. */
const MIGRATED_KEY = "orbyn.prefs.migrated";
const OLD_SIDEBAR_WIDTH = 208;

/** Set when this load's migration turned project folders on; the notice offers to undo it. */
let foldersMigrated = false;
const DRAFTS_KEY = "orbyn.drafts.v1";

const defaults: Prefs = {
  theme: "dark",
  panelOpen: false,
  panelTab: null,
  panelTabOrder: [...RIGHT_TAB_IDS],
  panelTabsClosed: [...CLOSED_AT_FIRST],
  panelAnimationMs: MOTION.slow,
  detailsOpen: true,
  terminalOpen: false,
  panelMaximized: false,
  panelWidth: null,
  sideChat: null,
  layouts: {},
  layoutKey: DRAFT_LAYOUT,
  sidebarCollapsed: false,
  sidebarWidth: DEFAULT_SIDEBAR_WIDTH,
  groups: {},
  projectFilter: [],
  projectFolders: true,
  projectGrouping: "separate",
  projectGroupingOverrides: {},
  multilineRows: false,
  threadSort: "updated_at",
  projectSort: "updated_at",
  modelHidden: [],
  modelOrder: [],
  effortDisplay: "label",
  preferredEditor: null,
  mathRendering: false,
  chatWidth: "comfortable",
  workspaceLayout: "console",
  wordWrap: false,
  timestampFormat: "locale",
  sendShortcut: "enter",
  followUpBehavior: "queue",
  diffLayout: "stacked",
  previewSearchEngine: "duckduckgo",
  proactivePanels: false,
  recentUrls: [],
  linkTarget: "system",
  autoFloatPreview: false,
  diffFilesCollapsed: false,
  diffIgnoreWhitespace: false,
  diffHideTests: false,
  lightPreset: null,
  darkPreset: null,
  customColors: {},
  typography: { uiFont: "", uiSize: 16, chatSize: 14, codeFont: "", codeSize: 13, terminalSize: 12 },
  contrast: "normal",
  fontSmoothing: true,
  persistentComposerContext: false,
  skillsInSlashMenu: false,
  planControls: true,
  contextMeter: true,
  stashes: {},
  threadNotifications: false,
  threadSound: false,
  inAppNotifications: false,
  taskRunNotifications: true,
  confirmUnpin: false,
  confirmArchive: false,
  confirmDelete: true,
  lastModel: null,
  draftProject: null,
  pullLinksInApp: true,
  detailsHidden: ["usage"],
  workingSection: true,
  environmentThemeAppliedAt: 0,
  environmentIdentification: "artwork",
  welcomeDone: false,
};

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
  /** Takes a surface out of this thread's tab row. The last tab stays; closing the open one opens its neighbour. */
  closePanelTab(tab: RightTab): void;
  /** Puts a closed surface back in the row, opens it and shows the panel. */
  reopenPanelTab(tab: RightTab): void;
  /** Moves a tab to `index` in the row's order. */
  movePanelTab(tab: RightTab, index: number): void;
  /** The default order, with every tab back in the row. */
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
  toggleDetails(): void;
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

const layoutDefaults: ThreadLayout = {
  panelTab: defaults.panelTab,
  panelTabOrder: defaults.panelTabOrder,
  panelTabsClosed: defaults.panelTabsClosed,
  detailsOpen: defaults.detailsOpen,
  terminalOpen: defaults.terminalOpen,
  panelMaximized: defaults.panelMaximized,
  panelWidth: defaults.panelWidth,
  sideChat: defaults.sideChat,
};

const currentLayout = (state: Prefs): ThreadLayout => ({
  panelTab: state.panelTab,
  panelTabOrder: state.panelTabOrder,
  panelTabsClosed: state.panelTabsClosed,
  detailsOpen: state.detailsOpen,
  terminalOpen: state.terminalOpen,
  panelMaximized: state.panelMaximized,
  panelWidth: state.panelWidth,
  sideChat: state.sideChat,
});

/** The newest layouts stay; object order is insertion order. */
function withLayout(layouts: Record<string, ThreadLayout>, key: string, layout: ThreadLayout) {
  const { [key]: _old, ...others } = layouts;
  const entries = Object.entries({ ...others, [key]: layout });
  return Object.fromEntries(entries.slice(-MAX_LAYOUTS));
}

const prefsOf = (state: Prefs & { desktopPanelOpen?: boolean | null }): Prefs => ({
  theme: state.theme,
  panelOpen: state.desktopPanelOpen ?? state.panelOpen,
  panelTab: state.panelTab,
  panelTabOrder: state.panelTabOrder,
  panelTabsClosed: state.panelTabsClosed,
  panelAnimationMs: state.panelAnimationMs,
  detailsOpen: state.detailsOpen,
  terminalOpen: state.terminalOpen,
  panelMaximized: state.panelMaximized,
  panelWidth: state.panelWidth,
  sideChat: state.sideChat,
  layouts: state.layouts,
  layoutKey: state.layoutKey,
  sidebarCollapsed: state.sidebarCollapsed,
  sidebarWidth: state.sidebarWidth,
  groups: state.groups,
  projectFilter: state.projectFilter,
  projectFolders: state.projectFolders,
  projectGrouping: state.projectGrouping,
  projectGroupingOverrides: state.projectGroupingOverrides,
  multilineRows: state.multilineRows,
  threadSort: state.threadSort,
  projectSort: state.projectSort,
  modelHidden: state.modelHidden,
  modelOrder: state.modelOrder,
  effortDisplay: state.effortDisplay,
  preferredEditor: state.preferredEditor,
  mathRendering: state.mathRendering,
  chatWidth: state.chatWidth,
  workspaceLayout: state.workspaceLayout,
  wordWrap: state.wordWrap,
  timestampFormat: state.timestampFormat,
  sendShortcut: state.sendShortcut,
  followUpBehavior: state.followUpBehavior,
  diffLayout: state.diffLayout,
  previewSearchEngine: state.previewSearchEngine,
  proactivePanels: state.proactivePanels,
  recentUrls: state.recentUrls,
  linkTarget: state.linkTarget,
  autoFloatPreview: state.autoFloatPreview,
  diffFilesCollapsed: state.diffFilesCollapsed,
  diffIgnoreWhitespace: state.diffIgnoreWhitespace,
  diffHideTests: state.diffHideTests,
  lightPreset: state.lightPreset,
  darkPreset: state.darkPreset,
  customColors: state.customColors,
  typography: state.typography,
  contrast: state.contrast,
  fontSmoothing: state.fontSmoothing,
  persistentComposerContext: state.persistentComposerContext,
  skillsInSlashMenu: state.skillsInSlashMenu,
  planControls: state.planControls,
  contextMeter: state.contextMeter,
  stashes: state.stashes,
  threadNotifications: state.threadNotifications,
  threadSound: state.threadSound,
  inAppNotifications: state.inAppNotifications,
  taskRunNotifications: state.taskRunNotifications,
  confirmUnpin: state.confirmUnpin,
  confirmArchive: state.confirmArchive,
  confirmDelete: state.confirmDelete,
  lastModel: state.lastModel,
  draftProject: state.draftProject,
  pullLinksInApp: state.pullLinksInApp,
  detailsHidden: state.detailsHidden,
  workingSection: state.workingSection,
  environmentThemeAppliedAt: state.environmentThemeAppliedAt,
  environmentIdentification: state.environmentIdentification,
  welcomeDone: state.welcomeDone,
});

const isPlainObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

/**
 * The frame's one-time migration: the previous default sidebar width (exactly 208) becomes
 * 224 and project folders turn on. It records itself under its own key, and only after the
 * migrated prefs were saved: a failed write leaves it to run again, without a notice.
 */
function migratePrefs(saved: Partial<Prefs>): Partial<Prefs> {
  try {
    if (localStorage.getItem(MIGRATED_KEY) === "1") return saved;
  } catch {
    return saved;
  }
  const existing = Object.keys(saved).length > 0;
  const next: Partial<Prefs> = { ...saved, projectFolders: true };
  if (saved.sidebarWidth === OLD_SIDEBAR_WIDTH) next.sidebarWidth = DEFAULT_SIDEBAR_WIDTH;
  try {
    localStorage.setItem(PREFS_KEY, JSON.stringify(next));
    localStorage.setItem(MIGRATED_KEY, "1");
  } catch {
    return saved;
  }
  // A first run starts on the new defaults; only someone with saved prefs is told.
  foldersMigrated = existing && saved.projectFolders !== true;
  return next;
}

/** Each thread's saved layout may predate the tab fields or name the removed Pull requests tab. */
function cleanLayouts(layouts: unknown): Record<string, ThreadLayout> {
  if (!isPlainObject(layouts)) return {};
  return Object.fromEntries(
    Object.entries(layouts as Record<string, unknown>)
      // A damaged entry is dropped; the thread starts from the defaults.
      .filter((entry): entry is [string, ThreadLayout] => isPlainObject(entry[1]))
      .map(([key, layout]) => {
        const tabs = cleanTabs(layout.panelTabOrder, layout.panelTabsClosed);
        const panelTab = RIGHT_TAB_IDS.includes(layout.panelTab as RightTab) ? layout.panelTab : null;
        // Open or closed is one global choice now; an older per-thread value must not override it.
        const { panelOpen: _global, ...rest } = layout as ThreadLayout & { panelOpen?: unknown };
        return [key, { ...rest, panelTab, panelTabOrder: tabs.order, panelTabsClosed: tabs.closed }];
      }),
  );
}

/** Saved preferences may predate a field or carry a removed value. */
function savedPrefs(): Partial<Prefs> {
  const stored = readJson<unknown>(PREFS_KEY);
  const saved = migratePrefs(isPlainObject(stored) ? (stored as Partial<Prefs>) : {});
  const panelTab = RIGHT_TAB_IDS.includes(saved.panelTab as RightTab) ? (saved.panelTab as RightTab) : null;
  const tabs = cleanTabs(saved.panelTabOrder, saved.panelTabsClosed);
  const grouping = saved.projectGrouping;
  const projectGrouping =
    grouping === "repository" || grouping === "repository_path" || grouping === "separate"
      ? grouping
      : defaults.projectGrouping;
  const projectGroupingOverrides = Object.fromEntries(
    Object.entries(saved.projectGroupingOverrides ?? {}).filter(
      ([, mode]) => mode === "repository" || mode === "repository_path" || mode === "separate",
    ),
  );
  const workspaceLayout = LAYOUT_CHOICES.includes(saved.workspaceLayout as LayoutChoice) ? saved.workspaceLayout : defaults.workspaceLayout;
  return { ...saved, panelOpen: saved.panelOpen === true, panelTab, panelTabOrder: tabs.order, panelTabsClosed: tabs.closed, layouts: cleanLayouts(saved.layouts),  projectGrouping, projectGroupingOverrides, workspaceLayout };
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
      set({
        fileReveal: { ...origin, path, line, nonce: ++navigationNonce },
        diffReveal: null,
        panelOpen: true,
        panelTab: "files",
        panelTabsClosed: get().panelTabsClosed.filter((id) => id !== "files"),
      });
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
      const row = panelTabOrder.filter((id) => !panelTabsClosed.includes(id));
      if (!row.includes(tab) || row.length < 2) return;
      const at = row.indexOf(tab);
      const shown = panelTab && row.includes(panelTab) ? panelTab : row[0];
      set({
        panelTabsClosed: [...panelTabsClosed, tab],
        ...(shown === tab ? { panelTab: row[at + 1] ?? row[at - 1] ?? null } : {}),
      });
      savePrefs();
    },
    reopenPanelTab(tab) {
      set({ panelTabsClosed: get().panelTabsClosed.filter((id) => id !== tab), panelOpen: true, panelTab: tab });
      savePrefs();
    },
    movePanelTab(tab, index) {
      const order = get().panelTabOrder.filter((id) => id !== tab);
      order.splice(Math.max(0, Math.min(index, order.length)), 0, tab);
      set({ panelTabOrder: order });
      savePrefs();
    },
    resetPanelTabs() {
      set({ panelTabOrder: [...RIGHT_TAB_IDS], panelTabsClosed: [...CLOSED_AT_FIRST] });
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
    toggleDetails() {
      set((state) => ({ detailsOpen: !state.detailsOpen }));
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

export function resolveTheme(choice: ThemeChoice): "light" | "dark" {
  if (choice !== "system") return choice;
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export function applyTheme(choice: ThemeChoice): void {
  const scheme = resolveTheme(choice);
  const root = document.documentElement;
  root.dataset.theme = scheme;
  const { lightPreset, darkPreset, customColors, typography, contrast, fontSmoothing, workspaceLayout } = useUi.getState();
  root.dataset.workspace = LAYOUT_CHOICES.includes(workspaceLayout) ? workspaceLayout : "console";
  const preset = presetById(scheme === "dark" ? darkPreset : lightPreset);
  if (preset) root.dataset.preset = preset.id;
  else delete root.dataset.preset;
  for (const [token] of THEME_TOKENS) {
    // A preset with a full token set in tokens.css is styled there; only custom colours override it.
    const value = customColors[token] ?? (preset?.tokens ? undefined : preset?.colors[token]);
    if (value) root.style.setProperty(token, value);
    else root.style.removeProperty(token);
  }
  root.dataset.contrast = contrast;
  root.dataset.smoothing = fontSmoothing ? "on" : "off";
  root.style.setProperty("zoom", typography.uiSize === 16 ? "" : String(typography.uiSize / 16));
  const set = (name: string, value: string) =>
    value ? root.style.setProperty(name, value) : root.style.removeProperty(name);
  set("--font-sans", typography.uiFont);
  set("--font-mono", typography.codeFont);
  root.style.setProperty("--chat-size", `${typography.chatSize}px`);
  root.style.setProperty("--code-size", `${typography.codeSize}px`);
}
