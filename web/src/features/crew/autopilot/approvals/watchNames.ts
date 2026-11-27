/**
 * The watch frame ids of Crew Designs (KC6) that draw in the Preview chunk: `watch-history` (the change table alone) and
 * `watch-could-not-start-reasons` (the same table over a watch's seven refused-start sentences). Every other KC6 frame
 * draws on the app's own routes (`#/crew/background`, and `#/crew/inbox` for watch-needsyou), so those ids are catalog
 * screens only and are not listed here.
 */
export const WATCH_FRAMES = ["watch-history", "watch-could-not-start-reasons"] as const;
export type WatchFrameId = (typeof WATCH_FRAMES)[number];
