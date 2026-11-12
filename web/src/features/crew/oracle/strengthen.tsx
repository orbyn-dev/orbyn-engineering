import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v22, frame `oracle-strengthen`. */
export function OracleStrengthen() {
  return (
    <Frame>
      <div className="cx-cwpg">
        <PageHead sub="Login redirect crew · Lead: Fix login redirect on Claude Code · 3 workers" acts />
        <div className="cx-cwtabs">
          <Seg kind="segx" on={6} items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} />
          <span className="cx-end">Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m</span>
        </div>
        <div className="cx-flcr">
          <span style={{ color: "var(--color-muted)" }}>All tasks</span>
          <Ic n="right" sm />
          <span style={{ color: "var(--color-muted)" }}>Add a rate limit to the login route</span>
          <Ic n="right" sm />
          <span style={{ color: "var(--color-muted)" }}>Report</span>
          <Ic n="right" sm />
          <b>Answer key</b>
        </div>
        <div className="cx-t14pg">
          <div className="cx-t10hd">
            <b>Answer key: Add a rate limit to the login route</b>
            <span className="cx-apchip">Independent</span>
            <span className="cx-st cx-info"><i className="cx-spin" aria-label="Working" role="img" />Strengthening · round 1 of 2</span>
            <span className="cx-t10sp"><Btn className="cx-ghost">Open the brief</Btn><Btn className="cx-ghost">Add a check</Btn></span>
          </div>
          <div className="cx-t14two">
            <div className="cx-t14tl">
              <div className="cx-h">
                <b style={{ color: "var(--color-text)" }}>Timeline</b>
                <span style={{ color: "var(--color-muted)" }}>Sat 7 Nov, as of 13:30</span>
              </div>
              <div><span className="cx-t">13:16</span><Ic n="x" sm /><span><b>Gate failed</b><small>All 18 mutants survived.</small></span></div>
              <div>
                <span className="cx-t">13:17</span>
                <Ic n="refresh" sm />
                <span>
                  <b>Sent back to the author</b>
                  <small>
                    Claude Code · Sonnet 5.5 was given the 18 survivors and asked for checks that fail on them and pass on the real code. It still doesn’t see the builder’s code, only the changed lines each mutant touches.
                  </small>
                </span>
              </div>
              <div>
                <span className="cx-t">13:31</span>
                <Ic n="check2" sm />
                <span><b>Round 1 of 2 · 3 new checks</b><small>Written. The gate will run again on all 6 checks.</small></span>
              </div>
            </div>
            <div className="cx-t14col">
              <div className="cx-fbx">
                <h6>This round</h6>
                <div className="cx-fkv">
                  <span>Author</span>
                  <b>Claude Code · Sonnet 5.5</b>
                  <span>Given</span>
                  <b>The 18 survivors, and the lines each one changed</b>
                  <span>Not given</span>
                  <b>The builder’s code as a whole, its notes and messages</b>
                  <span>Asked for</span>
                  <b>Checks that fail on a survivor and pass on the real code</b>
                </div>
              </div>
              <div className="cx-fbx">
                <h6>Then</h6>
                <p>The gate runs again on the 6 checks. If it kills at least one mutant it passes. If not, round 2, and after round 2 Crew asks you.</p>
              </div>
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
