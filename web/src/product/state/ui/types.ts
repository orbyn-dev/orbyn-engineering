import type { ThemeColors } from "../../../styles/themes";
import type { ProjectGroupingMode } from "../../../generated/ProjectGroupingMode";

export type ThemeChoice = "system" | "light" | "dark";
/** How the workspace is arranged: by project, by what needs you, or as one reading column. */
/** What a toast leads with: a thread needs you, finished or failed, or a copy was made. Others are text only. */
export type ToastKind = "needs" | "done" | "failed" | "copied";
/** A button on a toast, such as View. Using it closes the toast. */
export interface ToastAction { label: string; run(): void }

export type LayoutChoice = "console" | "ledger" | "reading";
export const LAYOUT_CHOICES: readonly LayoutChoice[] = ["console", "ledger", "reading"];
/** The panel's own surfaces. A file opened from the left bar is a tab too (`FileTabId`). */
export type BuiltinTab = "diff" | "activity" | "browser" | "side" | "tools";
/** A file tab's id: `file:` and an encoded target (see `features/panels/files/fileTab.ts`). */
export type FileTabId = `file:${string}`;
export type RightTab = BuiltinTab | FileTabId;
/** The panel's surfaces in their default order, the launcher's order. Terminal is the header button and the drawer, never a tab. */
export const RIGHT_TAB_IDS: readonly BuiltinTab[] = ["diff", "browser", "activity", "side", "tools"];

export const isFileTab = (id: unknown): id is FileTabId => typeof id === "string" && id.length > 5 && id.startsWith("file:");
export const isPanelTab = (id: unknown): id is RightTab => isFileTab(id) || RIGHT_TAB_IDS.includes(id as BuiltinTab);

/**
 * A saved order or closed set may be stale: unknown and repeated ids go (a Files tab saved before the left-bar
 * explorer goes with them), and missing surfaces return at the end, closed. A file tab in the order is open:
 * only a surface of the panel's own is ever "closed", a closed file tab is simply gone.
 */
export function cleanTabs(order: unknown, closed: unknown): { order: RightTab[]; closed: BuiltinTab[] } {
  const valid = (list: unknown): RightTab[] =>
    (Array.isArray(list) ? list : []).filter((id, at, all): id is RightTab => isPanelTab(id) && all.indexOf(id) === at);
  const saved = valid(order);
  const shut = valid(closed).filter((id): id is BuiltinTab => !isFileTab(id));
  // Every tab starts closed, so a thread's side panel starts with none; a saved row from before a tab existed has not seen it, so it starts closed too.
  for (const id of RIGHT_TAB_IDS) if (!saved.includes(id) && !shut.includes(id)) shut.push(id);
  return { order: [...saved, ...RIGHT_TAB_IDS.filter((id) => !saved.includes(id))], closed: shut };
}
export type LeftBarView = "threads" | "files";
/** The order with `tab` in it: a file tab not yet open joins the end. */
export const withTab = (order: readonly RightTab[], tab: RightTab): RightTab[] => (order.includes(tab) ? [...order] : [...order, tab]);

/** The default order: the panel's own surfaces as listed, then the open files as they were. */
export const defaultOrder = (order: readonly RightTab[]): RightTab[] => [...RIGHT_TAB_IDS, ...order.filter(isFileTab)];

export type ContextTab = RightTab | "terminal";
export interface ThreadNavigationOrigin { threadId: string; cwd: string }

export interface Prefs {
  theme: ThemeChoice;
  /** The surface panel, one choice for every thread. It opens only when asked, or for
   * something the person triggered (a diff, file or pull request chip), at any width. */
  panelOpen: boolean;
  /** The surface showing; null when none is. */
  panelTab: RightTab | null;
  /** The tab row's order for this thread. */
  panelTabOrder: RightTab[];
  /** Surfaces closed in this thread's tab row; the "+" brings them back. */
  panelTabsClosed: BuiltinTab[];
  /** Panel and hero motion in ms, 0 to 400. 0 is off; the default is the --m-slow token. */
  panelAnimationMs: number;
  /** The terminal drawer under the conversation. */
  terminalOpen: boolean;
  panelMaximized: boolean;
  /** The panel's width for this thread; null follows the window. */
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
  /** The left bar's view: the thread list or the file explorer. Remembered for each person. */
  leftBarView: LeftBarView;
  /** Explorer rows the viewer opened (true) or folded (false); a row not here follows its default. */
  explorerOpen: Record<string, boolean>;
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
  /** Wraps long lines in the Changes panel; on unless View options says otherwise. */
  diffWrapLines: boolean;
  /** A Markdown file in Changes shows as the reader will see it, not as source. Remembered for the type. */
  diffMarkdownRendered: boolean;
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
  panelTabsClosed: BuiltinTab[];
  terminalOpen: boolean;
  panelMaximized: boolean;
  panelWidth: number | null;
  sideChat: string | null;
}

export const DRAFT_LAYOUT = "draft";
export type PaletteStart = "root" | "add-project" | "add-project-url" | "add-project-new" | "files" | "search";

export const DEFAULT_SIDEBAR_WIDTH = 224;
