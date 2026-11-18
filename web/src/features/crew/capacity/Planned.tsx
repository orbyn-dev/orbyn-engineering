import type { CSSProperties } from "react";

/** The "Planned" pill of the Crew designs: it marks what a frame draws that the app does not have yet. */
export function Planned({ why, style }: { why: string; style?: CSSProperties }) {
  return (
    <span className="cx-cwpv" style={{ height: "16px", ...style }} title={`Planned: ${why}`}>
      Planned
    </span>
  );
}

/** Why each planned piece is not built, in the words its pill gives. */
export const WHY = {
  editor: "the app has no editor for the limit, the sampling or the pause switch; nothing here is sent",
  gaps: "the app draws the banner only, no list of the merges that are not measured and no way to try them again",
  files: "the app draws no link from an area to its files",
  queue: "the Review queue is not in the beta, so there is no button to open it",
} as const;
