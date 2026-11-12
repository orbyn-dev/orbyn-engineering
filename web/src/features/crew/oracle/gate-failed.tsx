import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v22, frame `oracle-gate-failed`. */
export function OracleGateFailed() {
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
            <span className="cx-st cx-err"><i className="cx-sd" />Gate failed</span>
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
              <span className="cx-t10b">3 checks</span>
              <span className="cx-t10s">written before the builder finished</span>
              <span className="cx-t10f">tests/auth/limit.test.ts · tests/auth/headers.test.ts</span>
            </div>
            <div className="cx-t10c">
              <span className="cx-t10l">Mutation gate</span>
              <span className="cx-t10b">Failed</span>
              <span className="cx-t10s">Stryker · TypeScript</span>
              <span className="cx-t10f">0 of 18 mutants killed</span>
            </div>
          </div>
          <div className="cx-t14two">
            <div className="cx-t14tl">
              <div className="cx-h">
                <b style={{ color: "var(--color-text)" }}>Timeline</b>
                <span style={{ color: "var(--color-muted)" }}>Sat 7 Nov, as of 13:30</span>
              </div>
              <div>
                <span className="cx-t">12:44</span>
                <Ic n="check2" sm />
                <span><b>Answer key written, blind</b><small>3 checks from the brief and the contract facts by Claude Code · Sonnet 5.5.</small></span>
              </div>
              <div>
                <span className="cx-t">13:02</span>
                <Ic n="git" sm />
                <span><b>Builder finished</b><small>Codex · GPT-5.5 · commit 5d2e1a8 · 3 files · +96 −4.</small></span>
              </div>
              <div>
                <span className="cx-t">13:10</span>
                <Ic n="refresh" sm />
                <span><b>Mutation gate started</b><small>Stryker made 18 mutants and ran the 3 checks on each.</small></span>
              </div>
              <div>
                <span className="cx-t">13:16</span>
                <Ic n="x" sm />
                <span>
                  <b>Gate failed</b>
                  <small>All 18 mutants survived. The checks pass on broken code, so they can’t tell the change from a broken one.</small>
                </span>
              </div>
            </div>
            <div className="cx-t14col">
              <div className="cx-fbx cx-red">
                <h6>Not an oracle yet</h6>
                <p>The key passes whether the code is right or broken, so a pass would prove nothing. The task doesn’t pass on this key.</p>
              </div>
              <div className="cx-fbx">
                <h6>What happens next</h6>
                <div className="cx-fkv">
                  <span>Sent to</span>
                  <b>Claude Code · Sonnet 5.5, the author</b>
                  <span>With</span>
                  <b>The 18 survivors, as the brief for stronger checks</b>
                  <span>Rounds</span>
                  <b>Up to 2 · Proposed</b>
                  <span>Then</span>
                  <b>If the gate still fails, Crew asks you</b>
                </div>
              </div>
            </div>
          </div>
          <div className="cx-aptb" style={{ gridTemplateColumns: "minmax(0,2.2fr) minmax(0,1fr)" }}>
            <div className="cx-h">Check written by the author</div>
            <div className="cx-h">In this run</div>
            <div><b>Five failed logins in a minute lock the route for the rest of the minute</b><small>tests/auth/limit.test.ts</small></div>
            <div><b>0</b><small>kills no mutant on its own</small></div>
            <div><b>The sixth request in a minute gets 429 with a Retry-After header</b><small>tests/auth/limit.test.ts</small></div>
            <div><b>0</b><small>kills no mutant on its own</small></div>
            <div><b>The limit headers are on every response</b><small>tests/auth/headers.test.ts</small></div>
            <div><b>0</b><small>kills no mutant on its own</small></div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
