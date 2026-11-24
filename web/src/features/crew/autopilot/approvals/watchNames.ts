/**
 * The watch frame ids of Crew Designs (KC6) that draw in the Preview chunk. Only `watch-history` is a section of a page;
 * every other KC6 frame draws on the app's own routes (`#/crew/background`, and `#/crew/inbox` for watch-needsyou), so
 * those ids are catalog screens only and are not listed here.
 */
export const WATCH_FRAMES = ["watch-history"] as const;
export type WatchFrameId = (typeof WATCH_FRAMES)[number];
