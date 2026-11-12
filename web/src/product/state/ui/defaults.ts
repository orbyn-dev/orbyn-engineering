import { MOTION } from "../../../lib/motion";
import { DEFAULT_SIDEBAR_WIDTH, DRAFT_LAYOUT, RIGHT_TAB_IDS, type Prefs, type ThreadLayout } from "./types";

export const MAX_LAYOUTS = 200;
export const PREFS_KEY = "orbyn.prefs.v1";

export const defaults: Prefs = {
  theme: "dark",
  panelOpen: false,
  panelTab: null,
  panelTabOrder: [...RIGHT_TAB_IDS],
  panelTabsClosed: [...RIGHT_TAB_IDS],
  panelAnimationMs: MOTION.slow,
  terminalOpen: false,
  panelMaximized: false,
  panelWidth: null,
  sideChat: null,
  layouts: {},
  layoutKey: DRAFT_LAYOUT,
  sidebarCollapsed: false,
  sidebarWidth: DEFAULT_SIDEBAR_WIDTH,
  groups: {},
  leftBarView: "threads",
  explorerOpen: {},
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
  diffWrapLines: true,
  diffMarkdownRendered: false,
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

export const layoutDefaults: ThreadLayout = {
  panelTab: defaults.panelTab,
  panelTabOrder: defaults.panelTabOrder,
  panelTabsClosed: defaults.panelTabsClosed,
  terminalOpen: defaults.terminalOpen,
  panelMaximized: defaults.panelMaximized,
  panelWidth: defaults.panelWidth,
  sideChat: defaults.sideChat,
};

export const currentLayout = (state: Prefs): ThreadLayout => ({
  panelTab: state.panelTab,
  panelTabOrder: state.panelTabOrder,
  panelTabsClosed: state.panelTabsClosed,
  terminalOpen: state.terminalOpen,
  panelMaximized: state.panelMaximized,
  panelWidth: state.panelWidth,
  sideChat: state.sideChat,
});

/** The newest layouts stay; object order is insertion order. */
export function withLayout(layouts: Record<string, ThreadLayout>, key: string, layout: ThreadLayout) {
  const { [key]: _old, ...others } = layouts;
  const entries = Object.entries({ ...others, [key]: layout });
  return Object.fromEntries(entries.slice(-MAX_LAYOUTS));
}
