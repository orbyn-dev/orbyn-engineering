export const PREVIEW_SCREENS = ["account", "connect-planner", "pair-phone", "phone", "phone-offline"] as const;
export type PreviewScreenName = (typeof PREVIEW_SCREENS)[number];
