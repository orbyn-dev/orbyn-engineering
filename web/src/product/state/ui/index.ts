// Front door: the UI store, its saved preferences and the theme applier.
export { applyTheme, resolveTheme } from "./theme";
export { useUi } from "./store";
export {
  DEFAULT_SIDEBAR_WIDTH,
  DRAFT_LAYOUT,
  LAYOUT_CHOICES,
  RIGHT_TAB_IDS,
  cleanTabs,
  type ContextTab,
  type EnvironmentIdentification,
  type LayoutChoice,
  type PaletteStart,
  type RightTab,
  type StashedPrompt,
  type ThemeChoice,
  type ThreadLayout,
  type ThreadNavigationOrigin,
  type ToastAction,
  type ToastKind,
  type Typography,
} from "./types";
