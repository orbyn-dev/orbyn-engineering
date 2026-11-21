import { lazy } from "react";

/**
 * The attention item's design page has no service behind it yet. Nothing links to it: it opens by hash (`#/preview/attention`) and only
 * when the build-time flag `__ORBYN_PREVIEWS__` is true. In a production build the flag is false, so the lazy import is dead code and the
 * chunk, the route and the mock are never emitted.
 */
export function attentionPreviewRoute(hash: string): boolean {
  if (!__ORBYN_PREVIEWS__) return false;
  return /^#\/preview\/attention(?:\?[a-z&-]*)?$/.test(hash);
}

export const AttentionPreview = __ORBYN_PREVIEWS__ ? lazy(() => import("./AttentionPreview")) : null;
