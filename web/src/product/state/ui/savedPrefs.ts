import { canonicalFileTab } from "./fileTab";
import { readJson } from "../../../lib/storage";
import { DEFAULT_SIDEBAR_WIDTH, LAYOUT_CHOICES, cleanTabs, isPanelTab, type LayoutChoice, type Prefs, type ThreadLayout } from "./types";
import { PREFS_KEY, defaults } from "./defaults";

const MIGRATED_KEY = "orbyn.prefs.migrated";
const OLD_SIDEBAR_WIDTH = 208;

/** Set when this load's migration turned project folders on; the notice offers to undo it. */
export let foldersMigrated = false;

export const prefsOf = (state: Prefs & { desktopPanelOpen?: boolean | null }): Prefs => ({
  theme: state.theme,
  panelOpen: state.desktopPanelOpen ?? state.panelOpen,
  panelTab: state.panelTab,
  panelTabOrder: state.panelTabOrder,
  panelTabsClosed: state.panelTabsClosed,
  panelAnimationMs: state.panelAnimationMs,
  terminalOpen: state.terminalOpen,
  panelMaximized: state.panelMaximized,
  panelWidth: state.panelWidth,
  sideChat: state.sideChat,
  layouts: state.layouts,
  layoutKey: state.layoutKey,
  sidebarCollapsed: state.sidebarCollapsed,
  sidebarWidth: state.sidebarWidth,
  groups: state.groups,
  leftBarView: state.leftBarView,
  explorerOpen: state.explorerOpen,
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

/** A saved tab row with its file tabs under today's ids. */
const canonicalList = (list: unknown) => (Array.isArray(list) ? list.map(canonicalFileTab) : list);

/** Each thread's saved layout may predate the tab fields or name the removed Pull requests tab. */
function cleanLayouts(layouts: unknown): Record<string, ThreadLayout> {
  if (!isPlainObject(layouts)) return {};
  return Object.fromEntries(
    Object.entries(layouts as Record<string, unknown>)
      // A damaged entry is dropped; the thread starts from the defaults.
      .filter((entry): entry is [string, ThreadLayout] => isPlainObject(entry[1]))
      .map(([key, layout]) => {
        const tabs = cleanTabs(canonicalList(layout.panelTabOrder), layout.panelTabsClosed);
        const panelTab = isPanelTab(canonicalFileTab(layout.panelTab)) ? (canonicalFileTab(layout.panelTab) as ThreadLayout["panelTab"]) : null;
        // Open or closed is one global choice now; an older per-thread value must not override it.
        // The Details card became a popover that opens from the header button; its old open state is dropped.
        const { panelOpen: _global, detailsOpen: _card, ...rest } = layout as ThreadLayout & { panelOpen?: unknown; detailsOpen?: unknown };
        return [key, { ...rest, panelTab, panelTabOrder: tabs.order, panelTabsClosed: tabs.closed }];
      }),
  );
}

/** Saved preferences may predate a field or carry a removed value. */
export function savedPrefs(): Partial<Prefs> {
  const stored = readJson<unknown>(PREFS_KEY);
  const saved = migratePrefs(isPlainObject(stored) ? (stored as Partial<Prefs>) : {});
  // The favourites list was removed; an old save still carries it.
  delete (saved as Record<string, unknown>).modelFavorites;
  delete (saved as Record<string, unknown>).detailsOpen;
  // The card's collapsed state had a key of its own; the popover has no such state.
  try {
    sessionStorage.removeItem("orbyn.details.collapsed");
  } catch {
    // Not available here; nothing to remove.
  }
  const panelTab = isPanelTab(canonicalFileTab(saved.panelTab)) ? (canonicalFileTab(saved.panelTab) as Prefs["panelTab"]) : null;
  const tabs = cleanTabs(canonicalList(saved.panelTabOrder), saved.panelTabsClosed);
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
  const leftBarView = saved.leftBarView === "files" ? "files" : "threads";
  const explorerOpen = Object.fromEntries(
    Object.entries(isPlainObject(saved.explorerOpen) ? saved.explorerOpen : {}).filter(([, open]) => typeof open === "boolean"),
  );
  const workspaceLayout = LAYOUT_CHOICES.includes(saved.workspaceLayout as LayoutChoice) ? saved.workspaceLayout : defaults.workspaceLayout;
  return { ...saved, panelOpen: saved.panelOpen === true, panelTab, panelTabOrder: tabs.order, panelTabsClosed: tabs.closed, layouts: cleanLayouts(saved.layouts),  projectGrouping, projectGroupingOverrides, workspaceLayout, leftBarView, explorerOpen };
}
