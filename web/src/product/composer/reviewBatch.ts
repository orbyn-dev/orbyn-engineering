import { chipPrompt, type ComposerChip, type ReviewChip } from "./composerContext";

/** The review comments waiting above a thread's composer, in the order they were written. */
export const reviewDrafts = (chips: readonly ComposerChip[] | undefined): ReviewChip[] =>
  (chips ?? []).filter((chip): chip is ReviewChip => chip.kind === "review");

/** A comment's file as the batch names it: another repository's file under its name, the thread's own by its path. */
const fileOf = (draft: ReviewChip): string => (draft.repoName ? `${draft.repoName}/${draft.path}` : draft.path);

/** How many comments and how many distinct files a batch holds, for "N comments in M files". */
export function batchSummary(drafts: readonly ReviewChip[]): { comments: number; files: number } {
  return { comments: drafts.length, files: new Set(drafts.map(fileOf)).size };
}

/** The drafts grouped by file, files in the order their first comment was written. */
export function groupByFile(drafts: readonly ReviewChip[]): { path: string; drafts: ReviewChip[] }[] {
  const groups = new Map<string, ReviewChip[]>();
  for (const draft of drafts) groups.set(fileOf(draft), [...(groups.get(fileOf(draft)) ?? []), draft]);
  return [...groups].map(([path, list]) => ({ path, drafts: list }));
}

/**
 * The one message that carries every draft: a heading with the count, then each comment as the engine already
 * receives a review chip (file, line, text and the lines themselves), in the order they were written.
 */
export function reviewMessage(drafts: readonly ReviewChip[], branch: string | null): string {
  const heading = `Review comments${branch ? ` on ${branch}` : ""} (${drafts.length})`;
  return [heading, ...drafts.map(chipPrompt)].join("\n\n");
}
