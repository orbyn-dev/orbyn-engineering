import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v22, frame `oracle-tool-error`. */
export function OracleToolError() {
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
            <span className="cx-st cx-err"><i className="cx-sd" />Gate couldn’t run</span>
            <span className="cx-t10sp"><Btn className="cx-ghost">Open the brief</Btn><Btn className="cx-ghost">Add a check</Btn></span>
          </div>
          <div className="cx-cwbn cx-red" style={{ margin: "0" }}>
            <Ic n="alert" sm />
            <span>
              Couldn’t finish the mutation run. Stryker exited with code 1 after 4 of 18 mutants, so there is no result. Nothing was changed, and no partial result is counted.
            </span>
            <span className="cx-acts"><Btn className="cx-ghost">Copy the log</Btn><Btn className="cx-btn">Try again</Btn></span>
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
              <span className="cx-t10b">No result</span>
              <span className="cx-t10s">Stryker · TypeScript</span>
              <span className="cx-t10f">4 of 18 mutants run, then the tool stopped</span>
            </div>
          </div>
          <div className="cx-t14code">
            {"Error: the test runner stopped after mutant 4 of 18 (tests/auth/limit.test.ts: “The count starts again after the minute” was still running after 5000 ms).\nStryker exited with code 1."}
          </div>
          <div className="cx-fbx">
            <h6>What you can do</h6>
            <p>
              Try again, or open the key’s checks: one check waits out a real minute, so a mutant that stops the clock can hang the run. The task isn’t reported as passed on the key until the gate has run.
            </p>
          </div>
        </div>
      </div>
    </Frame>
  );
}
