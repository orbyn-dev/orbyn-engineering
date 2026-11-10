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
  panelTabsClosed: RightTab[];
  /** Panel and hero motion in ms, 0 to 400. 0 is off; the default is the --m-slow token. */
  panelAnimationMs: number;
  /** The floating details card on wide layouts. */
  detailsOpen: boolean;
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
export type PaletteStart = "root" | "add-project" | "add-project-url" | "add-project-new" | "files" | "search";

export const DEFAULT_SIDEBAR_WIDTH = 224;
