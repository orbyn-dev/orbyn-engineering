import { Frame } from "../shared/Frame";
import { Btn, PageHead, Seg } from "../shared/parts";
import { Ic } from "./icons";

/** Crew Designs, frame `flow-you-ready`: Flow: you, after the merge (Full, the default). */
export function FlowYouReady() {
  return (
    <Frame height={700}>
      <div className="cx-cwpg">
            <PageHead sub="Login redirect crew · Task: Use next in the auth redirect · 5 runs" acts />
            <div className="cx-cwtabs">
              <Seg kind="segx" items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} on={0} />
              <span className="cx-end">Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m</span>
            </div>
            <div className="cx-flcr">
              <span style={{ color: "var(--color-muted)" }}>All tasks</span>
              <Ic n="right" sm />
              <b>Use next in the auth redirect</b>
              <span className="cx-end"><span className="cx-fil" style={{ display: "inline-flex", alignItems: "center", gap: "6px", height: "26px", padding: "0 10px", borderRadius: "8px", background: "var(--color-sunken)" }}><Ic n="plus" sm />Add reviewer<span className="cx-cwpv">After the beta</span></span><span className="cx-fil" style={{ display: "inline-flex", alignItems: "center", gap: "6px", height: "26px", padding: "0 10px", borderRadius: "8px", background: "var(--color-sunken)" }}><Ic n="down" sm />Round 1</span></span>
            </div>
            <div className="cx-fl" style={{ height: "226px", width: "1110px" }}>
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
              <svg className="cx-fe" width="1110" height="226" aria-hidden="true">
                <path d="M93,146 H103" />
                <path d="M103,146 H112" />
                <path className="cx-aht" d="M107,142 L112,146 L107,150 Z" />
                <path d="M206,146 H216" />
                <path d="M216,146 H225" />
                <path className="cx-aht" d="M220,142 L225,146 L220,150 Z" />
                <path d="M319,146 H329" />
                <path d="M329,146 H338" />
                <path className="cx-aht" d="M333,142 L338,146 L333,150 Z" />
                <path d="M432,146 H442" />
                <path d="M442,102 H451" />
                <path className="cx-aht" d="M446,98 L451,102 L446,106 Z" />
                <path d="M442,190 H451" />
                <path className="cx-aht" d="M446,186 L451,190 L446,194 Z" />
                <path d="M442,102 V190" />
                <path d="M545,102 H555" />
                <path d="M545,190 H555" />
                <path d="M555,146 H564" />
                <path className="cx-aht" d="M559,142 L564,146 L559,150 Z" />
                <path d="M555,102 V190" />
                <path d="M658,146 H668" />
                <path d="M668,146 H677" />
                <path className="cx-aht" d="M672,142 L677,146 L672,150 Z" />
                <path d="M771,146 H781" />
                <path d="M781,146 H790" />
                <path className="cx-aht" d="M785,142 L790,146 L785,150 Z" />
                <path d="M884,146 H894" />
                <path d="M894,146 H903" />
                <path className="cx-aht" d="M898,142 L903,146 L898,150 Z" />
                <path className="cx-p" d="M997,146 H1007" />
                <path className="cx-p" d="M1007,146 H1016" />
                <path className="cx-aht" d="M1011,142 L1016,146 L1011,150 Z" />
              </svg>
              <span className="cx-fgt cx-done" style={{ left: "93px", top: "120px" }} title="Artifact: brief"><Ic n="brief" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "93px", top: "152px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "206px", top: "120px" }} title="Artifact: route"><Ic n="gate" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "206px", top: "152px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "319px", top: "120px" }} title="Artifact: diff"><Ic n="diff" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "319px", top: "152px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "432px", top: "120px" }} title="Artifact: checks"><Ic n="check2" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "432px", top: "152px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "545px", top: "120px" }} title="Artifact: findings"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "545px", top: "152px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "658px", top: "120px" }} title="Artifact: findings"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "658px", top: "152px" }} title="Decided by finding"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "771px", top: "120px" }} title="Artifact: findings"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "771px", top: "152px" }} title="Decided by finding"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "884px", top: "120px" }} title="Artifact: merge"><Ic n="branch" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "884px", top: "152px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "997px", top: "120px" }} title="Artifact: review"><Ic n="eye" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "997px", top: "152px" }} title="Decided by you"><Ic n="user" sm /></span>
              <div className="cx-fln" style={{ left: "0px", top: "106px", height: "80px" }}>
                <b>Brief</b>
                <span className="cx-s">Lead</span>
                <span className="cx-s">Opus 5.5</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Written</span>
              </div>
              <div className="cx-fln" style={{ left: "113px", top: "106px", height: "80px" }}>
                <b>Route</b>
                <span className="cx-s">Claude Code</span>
                <span className="cx-s">Sonnet 5.5</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Routed</span>
              </div>
              <div className="cx-fln" style={{ left: "226px", top: "106px", height: "80px" }}>
                <b>Build</b>
                <span className="cx-s">Claude Code</span>
                <span className="cx-s">Sonnet 5.5</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Done</span>
              </div>
              <div className="cx-fln" style={{ left: "339px", top: "106px", height: "80px" }}>
                <b>Checks</b>
                <span className="cx-s">3 of 3</span>
                <span className="cx-s">code</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Passed</span>
              </div>
              <div className="cx-fln" style={{ left: "452px", top: "62px", height: "80px" }}>
                <b>Correctness</b>
                <span className="cx-s">Codex</span>
                <span className="cx-s">GPT-5.5</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />No findings</span>
              </div>
              <div className="cx-fln" style={{ left: "452px", top: "150px", height: "80px" }}>
                <b>Performance</b>
                <span className="cx-s">OpenCode</span>
                <span className="cx-s">DeepSeek V4</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />No findings</span>
              </div>
              <div className="cx-fln" style={{ left: "565px", top: "106px", height: "80px" }}>
                <b>Security</b>
                <span className="cx-s">Codex</span>
                <span className="cx-s">GPT-5.5 · Lead</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />No findings</span>
              </div>
              <div className="cx-fln" style={{ left: "678px", top: "106px", height: "80px" }}><b>Finding check</b><span className="cx-s">No findings</span><span className="cx-st cx-ok"><i className="cx-sd" />Clear</span></div>
              <div className="cx-fln" style={{ left: "791px", top: "106px", height: "80px" }}>
                <b>Merge queue</b>
                <span className="cx-s">Re-checks</span>
                <span className="cx-s">combined</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Merged</span>
              </div>
              <div className="cx-fln" style={{ left: "904px", top: "106px", height: "80px" }}><b>Main</b><span className="cx-s">Watched</span><span className="cx-st cx-ok"><i className="cx-sd" />Healthy</span></div>
              <div className="cx-fln" style={{ left: "1017px", top: "106px", height: "80px" }}>
                <b>You</b>
                <span className="cx-s">Review</span>
                <span className="cx-s">after merge</span>
                <span className="cx-st"><i className="cx-sd" />Your queue</span>
              </div>
            </div>
            <div className="cx-fyou">
              <div className="cx-col">
                <h4>You: review what reached main<span className="cx-st"><i className="cx-sd" />Review by Sun 8 Nov, 14:32</span></h4>
                <div className="cx-fbx">
                  <h6>Merged by Crew at 14:32 on Sat 7 Nov 2026 · Medium risk</h6>
                  <div style={{ fontSize: "12px" }}>
                    <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0 18px" }}>
                      <div style={{ display: "flex", gap: "8px", alignItems: "center", padding: "3px 0", borderTop: "1px solid var(--color-border)" }}><span style={{ color: "var(--color-text)" }}><Ic n="ok" sm /></span><b style={{ fontWeight: 500, color: "var(--color-text)" }}>Checks</b><span style={{ color: "var(--color-muted)", marginLeft: "auto" }}>3 of 3 · commit a41c9e2</span></div>
                      <div style={{ display: "flex", gap: "8px", alignItems: "center", padding: "3px 0", borderTop: "1px solid var(--color-border)" }}><span style={{ color: "var(--color-text)" }}><Ic n="ok" sm /></span><b style={{ fontWeight: 500, color: "var(--color-text)" }}>Code review</b><span style={{ color: "var(--color-muted)", marginLeft: "auto" }}>2 lanes · 0 confirmed</span></div>
                      <div style={{ display: "flex", gap: "8px", alignItems: "center", padding: "3px 0", borderTop: "1px solid var(--color-border)" }}><span style={{ color: "var(--color-text)" }}><Ic n="ok" sm /></span><b style={{ fontWeight: 500, color: "var(--color-text)" }}>Security review</b><span style={{ color: "var(--color-muted)", marginLeft: "auto" }}>1 lane · 0 confirmed</span></div>
                      <div style={{ display: "flex", gap: "8px", alignItems: "center", padding: "3px 0", borderTop: "1px solid var(--color-border)" }}><span style={{ color: "var(--color-text)" }}><Ic n="ok" sm /></span><b style={{ fontWeight: 500, color: "var(--color-text)" }}>Finding check</b><span style={{ color: "var(--color-muted)", marginLeft: "auto" }}>nothing to check</span></div>
                      <div style={{ display: "flex", gap: "8px", alignItems: "center", padding: "3px 0", borderTop: "1px solid var(--color-border)" }}><span style={{ color: "var(--color-text)" }}><Ic n="ok" sm /></span><b style={{ fontWeight: 500, color: "var(--color-text)" }}>CI on the PR</b><span style={{ marginLeft: "auto" }}><span className="cx-cwpv">Planned</span></span></div>
                      <div style={{ display: "flex", gap: "8px", alignItems: "center", padding: "3px 0", borderTop: "1px solid var(--color-border)" }}><span style={{ color: "var(--color-text)" }}><Ic n="ok" sm /></span><b style={{ fontWeight: 500, color: "var(--color-text)" }}>Merge queue</b><span style={{ color: "var(--color-muted)", marginLeft: "auto" }}>re-checked combined</span></div>
                    </div>
                  </div>
                </div>
              </div>
              <div className="cx-col">
                <div className="cx-fbx">
                  <h6>Not verified</h6>
                  <p>Other callers of redirect() outside auth/. No check covers them. Model review is advisory and was not counted.</p>
                </div>
                <div className="cx-fbx">
                  <p>Nothing here waits on you. The merge already happened; your review can still turn it into a fix brief or a revert.</p>
                </div>
              </div>
              <div className="cx-acts3"><Btn className="cx-btn">Looks good</Btn><Btn className="cx-ghost">Comment</Btn><Btn className="cx-ghost">Revert</Btn></div>
            </div>
      </div>
    </Frame>
  );
}
