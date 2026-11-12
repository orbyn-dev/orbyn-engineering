import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v22, frame `oracle-not-measured`. */
export function OracleNotMeasured() {
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
          <span style={{ color: "var(--color-muted)" }}>Add a retry to the calendar feed</span>
          <Ic n="right" sm />
          <span style={{ color: "var(--color-muted)" }}>Report</span>
          <Ic n="right" sm />
          <b>Answer key</b>
        </div>
        <div className="cx-t14pg">
          <div className="cx-t10hd">
            <b>Answer key: Add a retry to the calendar feed</b>
            <span className="cx-apchip">Independent</span>
            <span className="cx-apchip">Not validated</span>
            <span className="cx-t10sp"><Btn className="cx-ghost">Open the brief</Btn><Btn className="cx-ghost">Add a check</Btn></span>
          </div>
          <div className="cx-t10g" style={{ gridTemplateColumns: "repeat(3,minmax(0,1fr))" }}>
            <div className="cx-t10c">
              <span className="cx-t10l">Author</span>
              <span className="cx-t10b">Claude Code · Sonnet 5.5</span>
              <span className="cx-t10s">Builder: Codex · GPT-5.5</span>
              <span className="cx-t10f">Another engine, another vendor · saw the brief and the contract facts, not the code</span>
            </div>
            <div className="cx-t10c">
              <span className="cx-t10l">Answer key</span>
              <span className="cx-t10b">4 checks</span>
              <span className="cx-t10s">written before the builder finished</span>
              <span className="cx-t10f">tests/feed/retry_test.go</span>
            </div>
            <div className="cx-t10c">
              <span className="cx-t10l">Mutation gate</span>
              <span className="cx-t10b">Not measured</span>
              <span className="cx-t10s">Go · no tool Crew supports yet</span>
              <span className="cx-t10f">Nothing was run, so there is no result to show</span>
            </div>
          </div>
          <div className="cx-t14two">
            <div className="cx-t14tl">
              <div className="cx-h">
                <b style={{ color: "var(--color-text)" }}>Timeline</b>
                <span style={{ color: "var(--color-muted)" }}>Sat 7 Nov, as of 13:30</span>
              </div>
              <div>
                <span className="cx-t">10:12</span>
                <Ic n="check2" sm />
                <span>
                  <b>Answer key written, blind</b>
                  <small>4 checks from the brief and the contract facts by Claude Code · Sonnet 5.5. The author was not given the code.</small>
                </span>
              </div>
              <div>
                <span className="cx-t">10:40</span>
                <Ic n="git" sm />
                <span><b>Builder finished</b><small>Codex · GPT-5.5 · commit 91ac3d0 · 2 files · +58 −3 · Go.</small></span>
              </div>
              <div>
                <span className="cx-t">10:41</span>
                <Ic n="info" sm />
                <span>
                  <b>Mutation gate: Not measured</b>
                  <small>Crew has no mutation tool for Go yet, so it made no mutants. The key is independent; it is not validated.</small>
                </span>
              </div>
            </div>
            <div className="cx-t14col">
              <div className="cx-fbx">
                <h6>What Not measured means</h6>
                <div className="cx-fkv">
                  <span>Gate</span>
                  <b>Not run</b>
                  <span>Key</span>
                  <b>Independent, not validated</b>
                  <span>Task</span>
                  <b>Can pass on the key; the label stays</b>
                </div>
                <p style={{ margin: "6px 0 0" }}>It doesn’t mean the checks are good or bad. Nothing measured them.</p>
              </div>
              <div className="cx-t13row"><Btn className="cx-ghost">See supported languages</Btn></div>
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
