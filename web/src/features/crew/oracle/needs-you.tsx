import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v22, frame `oracle-needs-you`. */
export function OracleNeedsYou() {
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
            <span className="cx-st cx-warn"><i className="cx-sd" />Needs you</span>
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
                <span className="cx-t">13:31</span>
                <Ic n="refresh" sm />
                <span><b>Round 1 of 2</b><small>3 checks added. The gate still killed no mutant.</small></span>
              </div>
              <div>
                <span className="cx-t">13:52</span>
                <Ic n="refresh" sm />
                <span><b>Round 2 of 2</b><small>2 more checks added. The gate still killed no mutant.</small></span>
              </div>
              <div className="cx-am">
                <span className="cx-t">13:58</span>
                <Ic n="alert" sm />
                <span><b>Crew needs you</b><small>The gate failed after 2 rounds. Crew doesn’t go on without your say.</small></span>
              </div>
            </div>
            <div className="cx-t14col">
              <div className="cx-fbx cx-amber">
                <h6>The key is not an oracle yet</h6>
                <p>After 2 rounds the checks still pass on every broken version of the code. Choose what to do. Nothing was merged.</p>
                <div style={{ display: "flex", flexDirection: "column", gap: "8px", marginTop: "8px" }}>
                  <div className="cx-t14opt cx-on">
                    <Ic n="check2" sm />
                    <span>
                      <b>Ask another engine to write the key</b>
                      <small>OpenCode · DeepSeek V4 starts from the brief and the contract facts, blind, like the first author.</small>
                    </span>
                  </div>
                  <div className="cx-t14opt">
                    <Ic n="edit" sm />
                    <span><b>Edit the checks yourself</b><small>Open the checks in an editor. The gate runs again when you save.</small></span>
                  </div>
                  <div className="cx-t14opt">
                    <Ic n="minus" sm />
                    <span>
                      <b>Continue without the gate</b>
                      <small>The task is labelled “Answer key not validated” wherever it is shown, and the choice is logged.</small>
                    </span>
                  </div>
                  <div className="cx-t14opt"><Ic n="stop" sm /><span><b>Stop the task</b><small>Nothing is merged.</small></span></div>
                </div>
                <div style={{ display: "flex", gap: "8px", marginTop: "8px" }}>
                  <Btn className="cx-btn">Ask OpenCode to write it</Btn>
                  <Btn className="cx-ghost">Choose another option</Btn>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
