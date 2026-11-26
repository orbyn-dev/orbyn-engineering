import { useMemo } from "react";
import { slugify } from "../../../lib/format";
import { useWorkspace } from "../../../product/state/workspace";

export type OpenMenu = { kind: "checkout"; x: number; y: number };

export interface ModelChoice {
  engine: string;
  model: string | null;
}

export const MAX_PREVIOUS_WORKTREES = 5;

export const useThreadList = () => {
  const threads = useWorkspace((state) => state.threads);
  return useMemo(() => Object.values(threads), [threads]);
};


/** A worktree purpose nothing else has, so each background send gets its own checkout. */
export const uniquePurpose = (base: string) => [slugify(base) || "thread", crypto.randomUUID().slice(0, 6)].join("-");

/**
 * The seed a draft hands the service for the new worktree's branch name: the typed goal
 * with its whitespace normalized, cut to 50 characters plus `...` when longer, so 53
 * characters at most. It names the branch only and never becomes the thread title, which
 * stays "New thread" so the first message still generates it.
 */
export const branchNameSeed = (text: string): string => {
  const normalized = text.trim().replace(/\s+/gu, " ");
  const characters = Array.from(normalized);
  return characters.length <= 50 ? normalized : `${characters.slice(0, 50).join("")}...`;
};
