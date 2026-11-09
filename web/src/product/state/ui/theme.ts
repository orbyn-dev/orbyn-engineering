import { presetById, THEME_TOKENS } from "../../../styles/themes";
import { LAYOUT_CHOICES, type ThemeChoice } from "./types";
import { useUi } from "./store";

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
