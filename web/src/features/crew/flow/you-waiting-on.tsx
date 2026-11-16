import { Frame } from "../shared/Frame";
import { Btn, Seg } from "../shared/parts";
import { FlowHead } from "./FlowHead";
import { Ic } from "./icons";

/** Crew Designs, frame `flow-you-waiting-on`: Flow: waiting for you before the merge (Autopilot On). */
export function FlowYouWaitingOn() {
  return (
    <Frame height={700}>
      <div className="cx-cwpg">
            <FlowHead sub="Login redirect crew · Task: Rotate the signing key · 5 runs" acts />
            <div className="cx-cwtabs">
              <Seg kind="segx" items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} on={0} />
              <span className="cx-end">Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m<span className="cx-cwpv" title="Planned: no Flow route reports usage yet">Planned</span></span>
            </div>
            <div className="cx-flcr">
              <span style={{ color: "var(--color-muted)" }}>All tasks</span>
              <Ic n="right" sm />
              <b>Rotate the signing key</b>
              <span className="cx-end"><span className="cx-fil" style={{ display: "inline-flex", alignItems: "center", gap: "6px", height: "26px", padding: "0 10px", borderRadius: "8px", background: "var(--color-sunken)" }}><Ic n="plus" sm />Add reviewer<span className="cx-cwpv">After the beta</span></span><span className="cx-fil" style={{ display: "inline-flex", alignItems: "center", gap: "6px", height: "26px", padding: "0 10px", borderRadius: "8px", background: "var(--color-sunken)" }}><Ic n="down" sm />Round 1</span></span>
            </div>
            <div className="cx-fl" style={{ height: "216px", width: "1110px" }}>
              <span className="cx-cwpv" style={{ position: "absolute", left: "0", top: "0", margin: "0", zIndex: 1 }} title="The wide graph is Planned; the app’s zoom is the stage list">Planned</span>
              <span className="cx-fh" style={{ left: "0px", top: "22px", width: "93px" }}><Ic n="brief" sm />Brief</span>
              <span className="cx-fh" style={{ left: "113px", top: "22px", width: "93px" }}><Ic n="gate" sm />Route</span>
              <span className="cx-fh" style={{ left: "226px", top: "22px", width: "93px" }}><Ic n="wrench" sm />Build</span>
              <span className="cx-fh" style={{ left: "339px", top: "22px", width: "93px" }}><Ic n="check2" sm />Checks</span>
              <span className="cx-fh" style={{ left: "452px", top: "22px", width: "93px" }}><Ic n="eye" sm />Code review</span>
              <span className="cx-fh" style={{ left: "565px", top: "22px", width: "93px" }}><Ic n="shield" sm />Security review</span>
              <span className="cx-fh" style={{ left: "678px", top: "22px", width: "93px" }}><Ic n="scan" sm />Finding check</span>
              <span className="cx-fh" style={{ left: "791px", top: "22px", width: "93px" }}><Ic n="branch" sm />Merge queue</span>
              <span className="cx-fh" style={{ left: "904px", top: "22px", width: "93px" }}><Ic n="git" sm />Main</span>
              <span className="cx-fh" style={{ left: "1017px", top: "22px", width: "93px" }}><Ic n="user" sm />You</span>
              <span className="cx-fg" style={{ left: "113px", top: "0", width: "206px" }}>Build</span>
              <span className="cx-fg" style={{ left: "339px", top: "0", width: "432px" }}>Prove: separate runs, no limit</span>
              <span className="cx-fg" style={{ left: "791px", top: "0", width: "206px" }}>Land: no human wait</span>
              <span className="cx-fg" style={{ left: "1017px", top: "0", width: "93px" }}>You</span>
              <svg className="cx-fe" width="1110" height="216" aria-hidden="true">
                <path d="M93,134 H103" />
                <path d="M103,134 H112" />
                <path className="cx-aht" d="M107,130 L112,134 L107,138 Z" />
                <path d="M206,134 H216" />
                <path d="M216,134 H225" />
                <path className="cx-aht" d="M220,130 L225,134 L220,138 Z" />
                <path d="M319,134 H329" />
                <path d="M329,134 H338" />
                <path className="cx-aht" d="M333,130 L338,134 L333,138 Z" />
                <path d="M432,134 H442" />
                <path d="M442,90 H451" />
                <path className="cx-aht" d="M446,86 L451,90 L446,94 Z" />
                <path d="M442,178 H451" />
                <path className="cx-aht" d="M446,174 L451,178 L446,182 Z" />
                <path d="M442,90 V178" />
                <path d="M545,90 H555" />
                <path d="M545,178 H555" />
                <path d="M555,134 H564" />
                <path className="cx-aht" d="M559,130 L564,134 L559,138 Z" />
                <path d="M555,90 V178" />
                <path d="M658,134 H668" />
                <path d="M668,134 H677" />
                <path className="cx-aht" d="M672,130 L677,134 L672,138 Z" />
                <path className="cx-p" d="M771,134 H781" />
                <path className="cx-p" d="M781,134 H790" />
                <path className="cx-aht" d="M785,130 L790,134 L785,138 Z" />
                <path className="cx-p" d="M884,134 H894" />
                <path className="cx-p" d="M894,134 H903" />
                <path className="cx-aht" d="M898,130 L903,134 L898,138 Z" />
                <path className="cx-p" d="M997,134 H1007" />
                <path className="cx-p" d="M1007,134 H1016" />
                <path className="cx-aht" d="M1011,130 L1016,134 L1011,138 Z" />
              </svg>
              <span className="cx-fgt cx-done" style={{ left: "93px", top: "108px" }} title="Artifact: brief"><Ic n="brief" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "93px", top: "140px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "206px", top: "108px" }} title="Artifact: route"><Ic n="gate" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "206px", top: "140px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "319px", top: "108px" }} title="Artifact: diff"><Ic n="diff" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "319px", top: "140px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "432px", top: "108px" }} title="Artifact: checks"><Ic n="check2" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "432px", top: "140px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "545px", top: "108px" }} title="Artifact: findings"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "545px", top: "140px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "658px", top: "108px" }} title="Artifact: findings"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "658px", top: "140px" }} title="Decided by finding"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-need" style={{ left: "771px", top: "108px" }} title="Artifact: findings"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-need" style={{ left: "771px", top: "140px" }} title="Decided by you"><Ic n="user" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "884px", top: "108px" }} title="Artifact: merge"><Ic n="branch" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "884px", top: "140px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "997px", top: "108px" }} title="Artifact: review"><Ic n="eye" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "997px", top: "140px" }} title="Decided by you"><Ic n="user" sm /></span>
              <div className="cx-fln" style={{ left: "0px", top: "94px", height: "80px" }}>
                <b>Brief</b>
                <span className="cx-s">Lead</span>
                <span className="cx-s">Opus 5.5</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Written</span>
              </div>
              <div className="cx-fln" style={{ left: "113px", top: "94px", height: "80px" }}>
                <b>Route</b>
                <span className="cx-s">Claude Code</span>
                <span className="cx-s">Sonnet 5.5</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Routed</span>
              </div>
              <div className="cx-fln" style={{ left: "226px", top: "94px", height: "80px" }}>
                <b>Build</b>
                <span className="cx-s">Claude Code</span>
                <span className="cx-s">Sonnet 5.5</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Done</span>
              </div>
              <div className="cx-fln" style={{ left: "339px", top: "94px", height: "80px" }}>
                <b>Checks</b>
                <span className="cx-s">3 of 3</span>
                <span className="cx-s">code</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Passed</span>
              </div>
              <div className="cx-fln" style={{ left: "452px", top: "50px", height: "80px" }}>
                <b>Correctness</b>
                <span className="cx-s">Codex</span>
                <span className="cx-s">GPT-5.5</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />No findings</span>
              </div>
              <div className="cx-fln" style={{ left: "452px", top: "138px", height: "80px" }}>
                <b>Performance</b>
                <span className="cx-s">OpenCode</span>
                <span className="cx-s">DeepSeek V4</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />No findings</span>
              </div>
              <div className="cx-fln" style={{ left: "565px", top: "94px", height: "80px" }}>
                <b>Security</b>
                <span className="cx-s">Codex</span>
                <span className="cx-s">GPT-5.5 · Lead</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />No findings</span>
              </div>
              <div className="cx-fln" style={{ left: "678px", top: "94px", height: "80px" }}><b>Finding check</b><span className="cx-s">No findings</span><span className="cx-st cx-ok"><i className="cx-sd" />Clear</span></div>
              <div className="cx-fln cx-cpend" style={{ left: "791px", top: "94px", height: "80px" }}><b>Merge queue</b><span className="cx-s">Waits for you</span><span className="cx-st"><i className="cx-sd" />Waiting</span></div>
              <div className="cx-fln cx-cpend" style={{ left: "904px", top: "94px", height: "80px" }}><b>Main</b><span className="cx-s">Watched</span><span className="cx-st"><i className="cx-sd" />Waiting</span></div>
              <div className="cx-fln cx-cneeds" style={{ left: "1017px", top: "94px", height: "80px" }}>
                <b>You</b>
                <span className="cx-s">Review</span>
                <span className="cx-s">before merge</span>
                <span className="cx-st cx-warn"><i className="cx-sd" />Needs you</span>
              </div>
              <span className="cx-flab cx-a" style={{ left: "781px", top: "56px" }}>Waits for you before the merge</span>
            </div>
            <div className="cx-fyou">
              <div className="cx-col">
                <h4>High risk: waiting for you before the merge<span className="cx-st cx-warn"><i className="cx-sd" />Needs you</span></h4>
                <div className="cx-fbx cx-amber">
                  <h6>Autopilot is On for this repo</h6>
                  <p>Medium and Low risk changes merge by themselves. High risk waits for you here, and nothing else is blocked.</p>
                </div>
                <div className="cx-fbx">
                  <h6>Why High</h6>
                  <div className="cx-fkv">
                    <span>Paths</span>
                    <b>auth/login.ts, keys/rotate.ts</b>
                    <span>Size</span>
                    <b>2 files · +64 −12</b>
                  </div>
                </div>
              </div>
              <div className="cx-col">
                <div className="cx-fbx">
                  <h6>Every stage before the merge is green</h6>
                  <div style={{ fontSize: "12px" }}>
                    <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0 18px" }}>
                      <div style={{ display: "flex", gap: "8px", alignItems: "center", padding: "3px 0", borderTop: "1px solid var(--color-border)" }}><span style={{ color: "var(--color-text)" }}><Ic n="ok" sm /></span><b style={{ fontWeight: 500, color: "var(--color-text)" }}>Checks</b><span style={{ color: "var(--color-muted)", marginLeft: "auto" }}>3 of 3 · commit a41c9e2</span></div>
                      <div style={{ display: "flex", gap: "8px", alignItems: "center", padding: "3px 0", borderTop: "1px solid var(--color-border)" }}><span style={{ color: "var(--color-text)" }}><Ic n="ok" sm /></span><b style={{ fontWeight: 500, color: "var(--color-text)" }}>Code review</b><span style={{ color: "var(--color-muted)", marginLeft: "auto" }}>2 lanes · 0 confirmed</span></div>
                      <div style={{ display: "flex", gap: "8px", alignItems: "center", padding: "3px 0", borderTop: "1px solid var(--color-border)" }}><span style={{ color: "var(--color-text)" }}><Ic n="ok" sm /></span><b style={{ fontWeight: 500, color: "var(--color-text)" }}>Security review</b><span style={{ color: "var(--color-muted)", marginLeft: "auto" }}>1 lane · 0 confirmed</span></div>
                      <div style={{ display: "flex", gap: "8px", alignItems: "center", padding: "3px 0", borderTop: "1px solid var(--color-border)" }}><span style={{ color: "var(--color-text)" }}><Ic n="ok" sm /></span><b style={{ fontWeight: 500, color: "var(--color-text)" }}>Finding check</b><span style={{ color: "var(--color-muted)", marginLeft: "auto" }}>nothing to check</span></div>
                      <div style={{ display: "flex", gap: "8px", alignItems: "center", padding: "3px 0", borderTop: "1px solid var(--color-border)" }}><span style={{ color: "var(--color-text)" }}><Ic n="ok" sm /></span><b style={{ fontWeight: 500, color: "var(--color-text)" }}>CI on the PR</b><span style={{ marginLeft: "auto" }}><span className="cx-cwpv">Planned</span></span></div>
                      <div style={{ display: "flex", gap: "8px", alignItems: "center", padding: "3px 0", borderTop: "1px solid var(--color-border)" }}><span style={{ color: "var(--color-text)" }}><Ic n="ok" sm /></span><b style={{ fontWeight: 500, color: "var(--color-text)" }}>Merge queue</b><span style={{ color: "var(--color-muted)", marginLeft: "auto" }}>waits for you</span></div>
                    </div>
                  </div>
                  <p style={{ marginTop: "4px" }}>Not verified: key rotation under load.</p>
                </div>
              </div>
              <div className="cx-acts3"><Btn className="cx-btn" title="The Review queue is not in this app yet">Review</Btn><Btn className="cx-ghost">Send back to Build</Btn><Btn className="cx-ghost">Switch this repo to Full</Btn></div>
            </div>
      </div>
    </Frame>
  );
}
