import { lazy } from "react";

/**
 * The command blocks' design page runs against the mock client, not the service. Nothing links to it: it opens by hash (`#/preview/command-blocks`) and
 * only when the build-time flag `__ORBYN_PREVIEWS__` is true. In a production build the flag is false, so the lazy import is dead
 * code and the chunk, the route and the mock are never emitted.
 */
export function commandBlocksPreviewRoute(hash: string): boolean {
  if (!__ORBYN_PREVIEWS__) return false;
  return /^#\/preview\/command-blocks(?:\?[a-z&-]*)?$/.test(hash);
}

export const CommandBlocksPreview = __ORBYN_PREVIEWS__ ? lazy(() => import("./CommandBlocksPreview")) : null;
