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
