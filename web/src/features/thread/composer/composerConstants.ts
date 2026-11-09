import type { StashedPrompt } from "../../../state/ui";
import type { ComposerChip } from "../../../lib/composerContext";
import type { Suggestion } from "./MentionMenu";

/** Past this a keystroke would lag behind the styling, so the text stays plain. */
export const STYLED_CHARS = 20_000;

export const NO_STASHES: StashedPrompt[] = [];
export const NO_CHIPS: ComposerChip[] = [];
export const effortButton = () => document.querySelector<HTMLElement>("[data-effort-trigger]");
export const LOCAL_COMMANDS: Suggestion[] = [
  { value: "plan", detail: "Switch to Plan" },
  { value: "default", detail: "Switch back to Build" },
  { value: "usage-limits", detail: "Show usage limits" },
  { value: "side", detail: "Ask in a side chat" },
  { value: "btw", detail: "Ask in a side chat" },
];
export const PLAN_COMMANDS = new Set(["plan", "default"]);
/** Below this the footer's controls fold into one menu. */
export const NARROW_BAR = 440;
export const WIDE = 1000;
export const MENU_LIFT = 150;
export const MAX_HEIGHT_RATIO = 0.4;
export const MENU_ROWS = 6;
