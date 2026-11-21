import { createContext } from "react";
import type { SettingsSubPage } from "../../product/state/route";

/** The page inside the open Settings page the address names, for the page that has one (the Theme editor in Appearance). */
export const SettingsSubPageContext = createContext<SettingsSubPage | null>(null);

/** What each sub-page is called, and the Settings page that holds it. */
export const SUBPAGES: Record<SettingsSubPage, { label: string; parent: "appearance" }> = {
  "theme-editor": { label: "Theme editor", parent: "appearance" },
};
