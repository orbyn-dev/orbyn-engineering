import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v22, frame `oracle-key`. */
export function OracleKey() {
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
            <span className="cx-st cx-ok"><i className="cx-sd" />Gate passed</span>
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
              <span className="cx-t10b">Passed</span>
              <span className="cx-t10s">Stryker · TypeScript</span>
              <span className="cx-t10f">15 of 18 mutants killed</span>
            </div>
          </div>
          <div className="cx-t14two">
            <div className="cx-t14tl">
              <div className="cx-h">
                <b style={{ color: "var(--color-text)" }}>Timeline</b>
                <span style={{ color: "var(--color-muted)" }}>Sat 7 Nov, as of 13:30</span>
              </div>
              <div>
                <span className="cx-t">12:40</span>
                <Ic n="brief" sm />
                <span><b>Brief written</b><small>The lead wrote the brief, the scope and the contract facts for the login route.</small></span>
              </div>
              <div>
                <span className="cx-t">12:41</span>
                <Ic n="branch" sm />
                <span>
                  <b>Two engines chosen</b>
                  <small>
                    Builder Codex · GPT-5.5. Key author Claude Code · Sonnet 5.5: another engine, another vendor. Crew chose both; you can change the author.
                  </small>
                </span>
              </div>
              <div>
                <span className="cx-t">12:44</span>
                <Ic n="check2" sm />
                <span>
                  <b>Answer key written, blind</b>
                  <small>6 checks from the brief and the contract facts. The author was not given the code, the builder’s notes or its worktree.</small>
                </span>
              </div>
              <div>
                <span className="cx-t">13:02</span>
                <Ic n="git" sm />
                <span><b>Builder finished</b><small>Codex · GPT-5.5 · commit 5d2e1a8 · 3 files · +96 −4. The builder never saw the key.</small></span>
              </div>
              <div>
                <span className="cx-t">13:10</span>
                <Ic n="refresh" sm />
                <span><b>Mutation gate started</b><small>Stryker made 18 mutants in auth/rate-limit.ts and auth/login.ts and ran the 6 checks on each.</small></span>
              </div>
              <div>
                <span className="cx-t">13:19</span>
                <Ic n="ok" sm />
                <span><b>Gate passed</b><small>The key killed 15 of 18 mutants. 3 survived and are listed so you can see what the checks don’t cover.</small></span>
              </div>
            </div>
            <div className="cx-t14col">
              <div className="cx-fbx">
                <h6>The gate</h6>
                <div className="cx-fkv">
                  <span>Rule</span>
                  <b>Fails when the key kills no mutant</b>
                  <span>Tool</span>
                  <b>Stryker · TypeScript</b>
                  <span>Mutants</span>
                  <b>18 made · 15 killed · 3 survived</b>
                  <span>Result</span>
                  <b><span className="cx-st cx-ok"><i className="cx-sd" />Passed</span></b>
                </div>
                <p style={{ margin: "6px 0 0" }}>
                  A passed gate means the checks can catch a break. It doesn’t mean the change is right, and it doesn’t say the 3 survivors are harmless.
                </p>
              </div>
              <div className="cx-fbx">
                <h6>Independence</h6>
                <div className="cx-fkv">
                  <span>Author</span>
                  <b>Claude Code · Sonnet 5.5</b>
                  <span>Builder</span>
                  <b>Codex · GPT-5.5</b>
                  <span>Vendors</span>
                  <b>Anthropic and OpenAI</b>
                  <span>Given to the author</span>
                  <b>The brief and the contract facts</b>
                  <span>Not given to the author</span>
                  <b>The code, the builder’s notes and messages, its worktree</b>
                </div>
              </div>
              <div className="cx-t13row"><Btn className="cx-ghost">See the 3 survivors</Btn><Btn className="cx-ghost">Change the author…</Btn></div>
            </div>
          </div>
          <h5 className="cx-t10h5">The 6 checks</h5>
          <div className="cx-aptb" style={{ gridTemplateColumns: "minmax(0,2.2fr) minmax(0,1fr)" }}>
            <div className="cx-h">Check written by the author</div>
            <div className="cx-h">In this run</div>
            <div><b>Five failed logins in a minute lock the route for the rest of the minute</b><small>tests/auth/limit.test.ts</small></div>
            <div><b>7</b><small>mutants killed</small></div>
            <div><b>The sixth request in a minute gets 429 with a Retry-After header</b><small>tests/auth/limit.test.ts</small></div>
            <div><b>6</b><small>mutants killed</small></div>
            <div><b>The count starts again after the minute</b><small>tests/auth/limit.test.ts</small></div>
            <div><b>3</b><small>mutants killed</small></div>
            <div><b>A different address is not limited</b><small>tests/auth/limit.test.ts</small></div>
            <div><b>2</b><small>mutants killed</small></div>
            <div><b>A successful login does not use up the limit</b><small>tests/auth/limit.test.ts</small></div>
            <div><b>2</b><small>mutants killed</small></div>
            <div><b>The limit headers are on every response</b><small>tests/auth/headers.test.ts</small></div>
            <div><b>0</b><small>kills no mutant on its own</small></div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
