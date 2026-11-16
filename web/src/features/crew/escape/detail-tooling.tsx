import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v20, frame `escape-detail-tooling`. */
export function EscapeDetailTooling() {
  return (
    <Frame>
      <div className="cx-cwpg">
        <PageHead sub="Login redirect crew · Lead: Fix login redirect on Claude Code · 3 workers" acts />
        <div className="cx-cwtabs">
          <Seg kind="segx" on={1} items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} />
          <span className="cx-end">Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m</span>
        </div>
        <div className="cx-flcr">
          <span style={{ color: "var(--color-muted)" }}>Dashboard</span>
          <Ic n="right" sm />
          <span style={{ color: "var(--color-muted)" }}>Escape log</span>
          <Ic n="right" sm />
          <b>Use next in the auth redirect</b>
        </div>
        <div className="cx-t14pg">
          <div className="cx-t10hd">
            <b>Use next in the auth redirect</b>
            <span className="cx-apchip">Found by you</span>
            <span className="cx-apchip">Tooling gap</span>
            <span className="cx-st cx-info"><i className="cx-spin" aria-label="Working" role="img" />Check being written</span>
            <span className="cx-t10sp"><Btn className="cx-ghost">Change the class…</Btn><Btn className="cx-ghost">Not an escape…</Btn></span>
          </div>
          <div className="cx-t14two">
            <div className="cx-t14tl">
              <div className="cx-h">
                <b style={{ color: "var(--color-text)" }}>Timeline</b>
                <span style={{ color: "var(--color-muted)" }}>Same day unless a date is shown</span>
              </div>
              <div>
                <span className="cx-t">Sat 7 Nov, 14:32</span>
                <Ic n="git" sm />
                <span>
                  <b>Merged to main</b>
                  <small>Use next in the auth redirect · Claude Code · Sonnet 5.5 · Medium risk · reviewed by 2 lanes, 2 vendors · c7f20b9</small>
                </span>
              </div>
              <div>
                <span className="cx-t">15:02</span>
                <Ic n="eye" sm />
                <span><b>Found by you</b><small>You commented on the review: the legacy login route has no rate limit now. It was not reverted.</small></span>
              </div>
              <div>
                <span className="cx-t">15:20</span>
                <Ic n="bug" sm />
                <span>
                  <b>Marked as an escape</b>
                  <small>From the item’s menu. It counted from this moment: auth/ then had 1 revert and 1 escape in 14 days.</small>
                </span>
              </div>
              <div>
                <span className="cx-t">15:36</span>
                <Ic n="shield" sm />
                <span><b>auth/ moved from Full to On</b><small>The circuit breaker used the count. Low and Medium still merge by themselves.</small></span>
              </div>
              <div>
                <span className="cx-t">15:48</span>
                <Ic n="tree" sm />
                <span>
                  <b>Classified: tooling gap</b>
                  <small>A missing limit shows up under real traffic, so no cheap test could catch it. Crew chose this. You can change it.</small>
                </span>
              </div>
              <div>
                <span className="cx-t">16:10</span>
                <Ic n="ok" sm />
                <span><b>Fix merged</b><small>Add a rate limit to the login route · Codex · GPT-5.5 · through the merge queue.</small></span>
              </div>
              <div>
                <span className="cx-t">16:30</span>
                <Ic n="refresh" sm />
                <span>
                  <b>Check being written</b>
                  <small>A repo rule that every login route is in the rate-limit inventory. It must fail on c7f20b9 before it is added.</small>
                </span>
              </div>
            </div>
            <div className="cx-t14col">
              <div className="cx-fbx">
                <h6>Why a test can’t help</h6>
                <p>
                  Whether a limit is missing shows up under real traffic, and a test that sends that traffic would be slow and flaky. Crew adds a rule instead, so the gap is closed in CI and listed here as a tooling gap.
                </p>
              </div>
              <div className="cx-fbx">
                <h6>The check, as a repo rule</h6>
                <div className="cx-fkv">
                  <span>Name</span>
                  <b>Every login route is in the rate-limit inventory</b>
                  <span>Where</span>
                  <b>A rule in CI, for changes that touch auth/</b>
                  <span>State</span>
                  <b><span className="cx-st cx-info"><i className="cx-spin" aria-label="Working" role="img" />Being written</span></b>
                  <span>Proof before it is added</span>
                  <b>It fails on c7f20b9 and passes after the fix</b>
                </div>
              </div>
              <div className="cx-fbx">
                <h6>Counted against</h6>
                <div className="cx-fkv">
                  <span>Engine and model</span>
                  <b>Claude Code · Sonnet 5.5</b>
                  <span>Area</span>
                  <b>auth/</b>
                  <span>This engine, last 60 days</span>
                  <b>2 escapes in 21 merges · 10%</b>
                  <span>In auth/, by this engine</span>
                  <b>2 escapes in 9 merges · too few for a rate</b>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
