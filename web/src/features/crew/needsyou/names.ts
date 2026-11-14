/** Frame ids of Crew Designs (group 9, Needs you) built here. Each opens at `#/preview/crew/<id>`. `needsyou-live` is the same inbox on the app's own routes. */
export const NEEDSYOU_FRAMES = [
  "needsyou-inbox",
  "needsyou-inbox-off",
  "needsyou-approval",
  "needsyou-empty",
  "needsyou-many",
  "needsyou-narrow-panel",
  "needsyou-budget",
  "needsyou-demoted",
  "needsyou-stop",
  "needsyou-background",
  "needsyou-source-error",
  "needsyou-live",
] as const;

export type NeedsYouFrameId = (typeof NEEDSYOU_FRAMES)[number];

/**
 * Frames of the group that are not built, and the data each is waiting for (the route does not exist, so nothing is drawn):
 * - needsyou-batch: batched questions from a worker (no route lists a worker's questions or takes answers).
 * - needsyou-conflict: conflicting board writes with their evidence (no route reads the board's facts or records a choice).
 */
export const NEEDSYOU_NOT_BUILT = ["needsyou-batch", "needsyou-conflict"] as const;
