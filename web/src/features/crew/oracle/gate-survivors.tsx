import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v22, frame `oracle-gate-survivors`. */
export function OracleGateSurvivors() {
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
          <span style={{ color: "var(--color-muted)" }}>Answer key</span>
          <Ic n="right" sm />
          <b>Survivors</b>
        </div>
        <div className="cx-t14pg">
          <div className="cx-t10hd">
            <b>Mutants that survived</b>
            <span className="cx-apchip">3 of 18</span>
            <span className="cx-st cx-ok"><i className="cx-sd" />Gate passed</span>
            <span className="cx-t10sp"><Btn className="cx-btn">Ask the author for checks that kill all 3</Btn></span>
          </div>
          <div className="cx-cwbn" style={{ margin: "0" }}>
            <Ic n="info" sm />
            <span><b>The gate passed: the key killed 15 of 18 mutants.</b>{" These 3 survived. A survivor is a break that every check missed."}</span>
          </div>
          <div className="cx-aptb" style={{ gridTemplateColumns: "70px minmax(0,1.1fr) minmax(0,1.5fr) minmax(0,2fr)" }}>
            <div className="cx-h">Mutant</div>
            <div className="cx-h">Where</div>
            <div className="cx-h">What was changed</div>
            <div className="cx-h">What no check covers</div>
            <div><b>m7</b></div>
            <div><b>auth/rate-limit.ts:41</b></div>
            <div><span className="cx-t14code" style={{ padding: "2px 6px", display: "inline-block" }}>{"count > limit  →  count >= limit"}</span></div>
            <div>
              <small style={{ fontSize: "12px", color: "var(--color-textSoft)" }}>
                No check sends exactly five requests, so the edge of the limit is not tested.
              </small>
            </div>
            <div><b>m12</b></div>
            <div><b>auth/rate-limit.ts:58</b></div>
            <div><span className="cx-t14code" style={{ padding: "2px 6px", display: "inline-block" }}>{"resetAt = now + 60_000  →  removed"}</span></div>
            <div>
              <small style={{ fontSize: "12px", color: "var(--color-textSoft)" }}>
                The “starts again” check waits out the minute with a fake clock that the mutant does not touch.
              </small>
            </div>
            <div><b>m16</b></div>
            <div><b>auth/login.ts:33</b></div>
            <div><span className="cx-t14code" style={{ padding: "2px 6px", display: "inline-block" }}>{"Retry-After: 60  →  Retry-After: 61"}</span></div>
            <div><small style={{ fontSize: "12px", color: "var(--color-textSoft)" }}>The check asserts that the header is there, not its value.</small></div>
          </div>
          <div className="cx-fbx">
            <h6>What a survivor means</h6>
            <p>
              Some survivors are harmless: a change no behaviour depends on. Crew doesn’t decide which. It lists them, and the author can write a check for any you choose. A new check is proved the same way: it must fail on the survivor and pass on the real code.
            </p>
          </div>
        </div>
      </div>
    </Frame>
  );
}
