// The merge method a dialog and its menu start on, and where the project's setting for it lives.
import { useAsync } from "../../../lib/useAsync";
import { pullsApi } from "../api";

/** The methods the merge dialog offers and the merge route accepts. */
export const MERGE_METHODS = ["squash", "merge", "rebase"] as const;
export type MergeMethod = (typeof MERGE_METHODS)[number];

/** A value the merge dialog can start on. */
export function isMergeMethod(value: unknown): value is MergeMethod {
  return typeof value === "string" && (MERGE_METHODS as readonly string[]).includes(value);
}

/**
 * The method the merge dialog and the page's merge menu start on. The project's merge method is the
 * default: the setting in Settings › Diffs, with the project's own override winning over the global
 * value. Settings' Ask ("") and a service that does not list the setting leave the dialog on Squash
 * and merge, which is where it has always started, and the other methods stay one choice away.
 */
export function startMergeMethod(configured: unknown): MergeMethod {
  return isMergeMethod(configured) ? configured : "squash";
}

/**
 * The project's configured merge method, read the way Settings reads it, so a repository's override
 * applies. Read fresh wherever a dialog opens, so a setting changed since the page loaded is obeyed.
 */
export function useMergeMethodSetting(projectId: string) {
  return useAsync(`pull-merge-method:${projectId}`, (signal) => pullsApi.mergeMethodSetting(projectId, signal));
}
