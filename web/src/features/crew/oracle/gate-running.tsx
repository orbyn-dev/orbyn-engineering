import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v22, frame `oracle-gate-running`. */
export function OracleGateRunning() {
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
            <span className="cx-st cx-info"><i className="cx-spin" aria-label="Working" role="img" />Gate running</span>
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
              <span className="cx-t10b">6 checks</span>
              <span className="cx-t10s">written before the builder finished</span>
              <span className="cx-t10f">tests/auth/limit.test.ts · tests/auth/headers.test.ts</span>
            </div>
            <div className="cx-t10c">
              <span className="cx-t10l">Mutation gate</span>
              <span className="cx-t10b">Running</span>
              <span className="cx-t10s">Stryker · TypeScript</span>
              <span className="cx-t10f">11 of 18 mutants run so far</span>
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
                <span><b>Answer key written, blind</b><small>6 checks from the brief and the contract facts. The author was not given the code.</small></span>
              </div>
              <div>
                <span className="cx-t">13:02</span>
                <Ic n="git" sm />
                <span><b>Builder finished</b><small>Codex · GPT-5.5 · commit 5d2e1a8 · 3 files · +96 −4.</small></span>
              </div>
              <div>
                <span className="cx-t">13:10</span>
                <Ic n="refresh" sm />
                <span>
                  <b>Mutation gate running</b>
                  <small>
                    Stryker made 18 mutants in auth/rate-limit.ts and auth/login.ts. 11 have been run so far: 9 killed, 2 survived. It will finish by itself; nothing needs you.
                  </small>
                </span>
              </div>
            </div>
            <div className="cx-t14col">
              <div className="cx-fbx">
                <h6>Counts so far</h6>
                <div className="cx-fkv">
                  <span>Mutants run</span>
                  <b>11 of 18</b>
                  <span>Killed</span>
                  <b>9</b>
                  <span>Survived</span>
                  <b>2</b>
                  <span>Not run yet</span>
                  <b>7</b>
                </div>
              </div>
              <div className="cx-fbx">
                <h6>What happens next</h6>
                <p>
                  When the last mutant has run, the gate is Passed (the key killed at least one mutant) or Failed (it killed none). A failed gate goes back to the author.
                </p>
              </div>
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
