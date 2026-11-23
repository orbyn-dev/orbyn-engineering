import { Ic } from "../../shared/parts";
import { useCrewHref } from "../../shared/nav";
import type { CrewOrigin } from "../../../../product/state/route";

/**
 * The Coverage link (Crew Designs v87, `entry-crew-coverage-link-*`): an underlined word and a chevron that open the coverage map.
 * `project` is the project the page shows, so the map opens for the same one; `from` names this page in the address, so the map can offer the way back to it. Opening reads the map on its own page and sends nothing.
 */
export function CoverageLink({ project, from }: { project: string; from: CrewOrigin }) {
  const href = useCrewHref();
  return (
    <a
      className="cx-t10r"
      href={href({ page: "coverage", project, from })}
      style={{ marginLeft: "0", whiteSpace: "nowrap", display: "inline-flex", alignItems: "center", gap: "2px", color: "inherit" }}
    >
      <span style={{ textDecoration: "underline", textUnderlineOffset: "2px" }}>Coverage</span>
      <Ic n="right" sm />
    </a>
  );
}
