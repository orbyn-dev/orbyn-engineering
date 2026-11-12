import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v22, frame `oracle-tool-missing`. */
export function OracleToolMissing() {
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
          <span style={{ color: "var(--color-muted)" }}>Handle the OAuth state mismatch</span>
          <Ic n="right" sm />
          <span style={{ color: "var(--color-muted)" }}>Report</span>
          <Ic n="right" sm />
          <b>Answer key</b>
        </div>
        <div className="cx-t14pg">
          <div className="cx-t10hd">
            <b>Answer key: Handle the OAuth state mismatch</b>
            <span className="cx-apchip">Independent</span>
            <span className="cx-apchip">Not validated</span>
            <span className="cx-t10sp"><Btn className="cx-ghost">Open the brief</Btn><Btn className="cx-ghost">Add a check</Btn></span>
          </div>
          <div className="cx-cwbn" style={{ margin: "0" }}>
            <Ic n="info" sm />
            <span>
              <b>Mutation gate: Not measured.</b>
              {" cargo-mutants isn’t installed in orbyn-engine, so no mutants were made. The key is still written by another engine."}
            </span>
            <span className="cx-acts"><Btn className="cx-btn">Show how to install</Btn></span>
          </div>
          <div className="cx-t10g" style={{ gridTemplateColumns: "repeat(3,minmax(0,1fr))" }}>
            <div className="cx-t10c">
              <span className="cx-t10l">Author</span>
              <span className="cx-t10b">Codex · GPT-5.5</span>
              <span className="cx-t10s">Builder: Claude Code · Sonnet 5.5</span>
              <span className="cx-t10f">Another engine, another vendor · saw the brief and the contract facts, not the code</span>
            </div>
            <div className="cx-t10c">
              <span className="cx-t10l">Answer key</span>
              <span className="cx-t10b">5 checks</span>
              <span className="cx-t10s">written before the builder finished</span>
              <span className="cx-t10f">crates/auth/tests/oauth_state.rs</span>
            </div>
            <div className="cx-t10c">
              <span className="cx-t10l">Mutation gate</span>
              <span className="cx-t10b">Not measured</span>
              <span className="cx-t10s">Rust · cargo-mutants not installed</span>
              <span className="cx-t10f">Nothing was run, so there is no result to show</span>
            </div>
          </div>
          <div className="cx-fbx">
            <h6>Install it</h6>
            <p>Run this in orbyn-engine, then choose Check again. Crew doesn’t run it for you.</p>
            <div className="cx-t14code">cargo install cargo-mutants</div>
            <div style={{ display: "flex", gap: "8px", marginTop: "6px" }}>
              <Btn className="cx-ghost">Copy the command</Btn>
              <Btn className="cx-btn">Check again</Btn>
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
