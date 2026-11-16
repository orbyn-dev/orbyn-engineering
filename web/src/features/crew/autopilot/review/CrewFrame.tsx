import type { ReactNode } from "react";
import { Button } from "../../../../ui/controls";
import { unavailable } from "./Parts";

export type CrewTool = "review" | "merge" | "digest";

/** The three Autopilot views this slice draws; the other tool tabs belong to other frames. */
export const TOOL_TARGET: Record<CrewTool, string> = {
  review: "crew/review-queue",
  merge: "crew/mergequeue-main",
  digest: "crew/digest-daily",
};

const PAGE_TABS = ["Flow", "Review", "Map", "Board", "Lanes", "Inbox", "Report"] as const;
export type CrewPageTab = (typeof PAGE_TABS)[number];

function go(target: string) {
  window.location.hash = `#/preview/${target}`;
}

/**
 * The Crew page as drawn around every Autopilot frame: header, page tabs, and (on Review) the tool tabs.
 * Only the tool tabs navigate; the other page tabs are other screens and stay inert here.
 */
export function CrewFrame({
  tab,
  tool,
  reviewCount,
  mergeCount,
  inbox = 4,
  health,
  crumbs,
  children,
}: {
  tab: CrewPageTab;
  tool?: CrewTool;
  reviewCount: number;
  /** Candidates in the Merge queue; shown on its tab when above zero. */
  mergeCount?: number;
  inbox?: number;
  health?: string;
  crumbs?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="crew-page">
      <p className="crew-notice" role="note">
        <strong>Sample data.</strong> Crew is not connected to this app yet, so nothing here changes a repository.
      </p>
      <header className="crew-ph">
        <h2>
          Crew<span className="crew-pv">Preview</span>
        </h2>
        <span className="crew-sub">Login redirect crew · Lead: Fix login redirect on Claude Code · 3 workers</span>
        <div className="crew-acts">
          <Button small {...unavailable}>
            Pause crew
          </Button>
          <Button small {...unavailable}>
            Stop crew
          </Button>
        </div>
      </header>
      <div className="crew-tabs">
        <div className="crew-seg" role="tablist" aria-label="Crew views">
          {PAGE_TABS.map((name) => {
            const raw = name === "Review" ? reviewCount : name === "Inbox" ? inbox : 0;
            const count = raw > 0 ? raw : undefined;
            const on = name === tab;
            return (
              <button
                key={name}
                type="button"
                role="tab"
                aria-selected={on}
                aria-disabled={on ? undefined : true}
                tabIndex={on ? 0 : -1}
                className={on ? "on" : undefined}
              >
                {count !== undefined ? `${name} · ${count}` : name}
              </button>
            );
          })}
        </div>
        <span className="crew-end">Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m</span>
      </div>
      {tool ? (
        <div className="crew-tool">
          <div className="crew-seg" role="tablist" aria-label="Autopilot views">
            <button type="button" role="tab" aria-selected={tool === "review"} className={tool === "review" ? "on" : undefined} onClick={() => go(TOOL_TARGET.review)}>
              {reviewCount > 0 ? `Review queue · ${reviewCount}` : "Review queue"}
            </button>
            <button type="button" role="tab" aria-selected={tool === "merge"} className={tool === "merge" ? "on" : undefined} onClick={() => go(TOOL_TARGET.merge)}>
              {mergeCount ? `Merge queue · ${mergeCount}` : "Merge queue"}
            </button>
            <button type="button" role="tab" aria-selected={tool === "digest"} className={tool === "digest" ? "on" : undefined} onClick={() => go(TOOL_TARGET.digest)}>
              Digest
            </button>
            {["Trust", "Dashboard", "Background"].map((name) => (
              <button key={name} type="button" role="tab" aria-selected={false} aria-disabled tabIndex={-1}>
                {name}
              </button>
            ))}
          </div>
          {health ? <span className="crew-health">{health}</span> : null}
        </div>
      ) : null}
      {crumbs ? <div className="crew-crumbs">{crumbs}</div> : null}
      {children}
    </div>
  );
}
