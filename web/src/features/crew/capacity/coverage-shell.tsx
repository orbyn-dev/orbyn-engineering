import type { ReactNode } from "react";
import { Frame } from "../shared/Frame";
import { Ic, PageHead, Seg } from "../shared/parts";
import { Planned } from "./Planned";

const TABS = ["Approvals", "Trust", "Background", "Inbox", "Flow", "Board", "Map", "Lanes"];

/**
 * The page around the Review coverage frames of Crew Designs v77: the Crew page head, its tabs and the breadcrumb, each
 * with the Planned pill the design gives them (the app draws the coverage page alone), then the page's own header.
 */
export function CoverageShell({ project, notMeasured, children }: { project: string; /** The gaps frame: a three-part crumb and no page header row. */ notMeasured?: boolean; children: ReactNode }) {
  return (
    <Frame>
      <div className="cx-cwpg">
        <PageHead sub="Login redirect crew · Lead: Fix login redirect on claude · 3 workers" acts />
        <div className="cx-cwtabs">
          <span className="cx-segx">
            {TABS.map((label) => (
              <span key={label}>{label}</span>
            ))}
            <span className="cx-on" aria-current="true">
              Review
              <Planned why="the beta has no Review tab; this view is drawn here and is reached another way" style={{ marginLeft: "6px", verticalAlign: "middle" }} />
            </span>
          </span>
          <span className="cx-end">claude 412k of 1M tokens · codex 62% of its weekly limit · 2h 14m</span>
        </div>
        <div className="cx-flcr">
          <span style={{ color: "var(--color-muted)" }}>Dashboard</span>
          <Ic n="right" sm />
          {notMeasured ? (
            <>
              <span style={{ color: "var(--color-muted)" }}>Review coverage</span>
              <Ic n="right" sm />
              <b>Not measured</b>
            </>
          ) : (
            <b>
              Review coverage
              <Planned why="the app draws the page alone, with no page head, tabs or breadcrumb" style={{ marginLeft: "6px" }} />
            </b>
          )}
        </div>
        <div className="cx-t14pg">
          {notMeasured ? null : (
            <div className="cx-t10hd">
              <b>Review coverage</b>
              <span className="cx-rtag cx-lo">Sample data</span>
              <span className="cx-rtag cx-lo">This device only</span>
              <span className="cx-t10sp">
                <span className="cx-t10sel"><Ic n="folder" sm />{project}</span>
                <Seg kind="cwseg" on={0} items={["Since provenance began"]} />
              </span>
            </div>
          )}
          {children}
        </div>
      </div>
    </Frame>
  );
}
