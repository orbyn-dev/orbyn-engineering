import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v22, frame `oracle-no-second-engine`. */
export function OracleNoSecondEngine() {
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
            <span className="cx-st cx-warn"><i className="cx-sd" />Needs you</span>
            <span className="cx-t10sp"><Btn className="cx-ghost">Open the brief</Btn></span>
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
                <span><b>Brief written</b><small>The lead wrote the brief and the contract facts for the login route.</small></span>
              </div>
              <div>
                <span className="cx-t">12:41</span>
                <Ic n="branch" sm />
                <span><b>Builder chosen</b><small>Codex · GPT-5.5. Crew looked for another engine to write the key and found none signed in.</small></span>
              </div>
              <div className="cx-am">
                <span className="cx-t">12:41</span>
                <Ic n="alert" sm />
                <span><b>Crew needs you</b><small>The task is waiting to start building. It doesn’t start on a key Crew can’t call independent.</small></span>
              </div>
            </div>
            <div className="cx-t14col">
              <div className="cx-fbx cx-amber">
                <h6>Crew needs a second engine to write the key</h6>
                <p>
                  Only Codex · GPT-5.5 is signed in, and it is building this task. An answer key from the same engine could share its mistakes, so Crew doesn’t call it independent.
                </p>
                <div style={{ display: "flex", flexDirection: "column", gap: "8px", marginTop: "8px" }}>
                  <div className="cx-t14opt cx-on">
                    <Ic n="user" sm />
                    <span><b>Sign in to another engine</b><small>Claude Code, OpenCode or Cursor. The key is written as soon as one is signed in.</small></span>
                  </div>
                  <div className="cx-t14opt">
                    <Ic n="eye" sm />
                    <span>
                      <b>Use the same engine, labelled</b>
                      <small>
                        A separate run of Codex writes the key. It is labelled “Same engine as builder: not independent” wherever it is shown. Crew asks each time.
                      </small>
                    </span>
                  </div>
                  <div className="cx-t14opt">
                    <Ic n="minus" sm />
                    <span>
                      <b>Continue without an independent key</b>
                      <small>The checks the builder writes decide pass or fail, as before. The choice is logged.</small>
                    </span>
                  </div>
                </div>
                <div style={{ display: "flex", gap: "8px", marginTop: "8px" }}>
                  <Btn className="cx-btn">Sign in to another engine</Btn>
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
