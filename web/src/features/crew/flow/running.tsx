import { Frame } from "../shared/Frame";
import { Btn, PageHead, Seg } from "../shared/parts";
import { Ic } from "./icons";

/** Crew Designs, frame `flow-running`: Flow: a review that is still running. */
export function FlowRunning() {
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
              <span className="cx-end"><span className="cx-fil" style={{ display: "inline-flex", alignItems: "center", gap: "6px", height: "26px", padding: "0 10px", borderRadius: "8px", background: "var(--color-sunken)" }}><Ic n="plus" sm />Add reviewer</span><span className="cx-fil" style={{ display: "inline-flex", alignItems: "center", gap: "6px", height: "26px", padding: "0 10px", borderRadius: "8px", background: "var(--color-sunken)" }}><Ic n="down" sm />Round 1</span></span>
            </div>
            <div className="cx-fl" style={{ height: "322px", width: "1110px" }}>
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
              <svg className="cx-fe" width="1110" height="322" aria-hidden="true">
                <path d="M93,192 H103" />
                <path d="M103,192 H112" />
                <path className="cx-aht" d="M107,188 L112,192 L107,196 Z" />
                <path d="M206,192 H216" />
                <path d="M216,192 H225" />
                <path className="cx-aht" d="M220,188 L225,192 L220,196 Z" />
                <path d="M319,192 H329" />
                <path d="M329,192 H338" />
                <path className="cx-aht" d="M333,188 L338,192 L333,196 Z" />
                <path d="M432,192 H442" />
                <path d="M442,104 H451" />
                <path className="cx-aht" d="M446,100 L451,104 L446,108 Z" />
                <path d="M442,192 H451" />
                <path className="cx-aht" d="M446,188 L451,192 L446,196 Z" />
                <path d="M442,280 H451" />
                <path className="cx-aht" d="M446,276 L451,280 L446,284 Z" />
                <path d="M442,104 V280" />
                <path className="cx-l" d="M545,104 H555" />
                <path className="cx-l" d="M545,192 H555" />
                <path className="cx-l" d="M545,280 H555" />
                <path className="cx-l" d="M555,192 H564" />
                <path className="cx-ahb" d="M559,188 L564,192 L559,196 Z" />
                <path className="cx-l" d="M555,104 V280" />
                <path className="cx-p" d="M658,192 H668" />
                <path className="cx-p" d="M668,192 H677" />
                <path className="cx-aht" d="M672,188 L677,192 L672,196 Z" />
                <path className="cx-p" d="M771,192 H781" />
                <path className="cx-p" d="M781,192 H790" />
                <path className="cx-aht" d="M785,188 L790,192 L785,196 Z" />
                <path className="cx-p" d="M884,192 H894" />
                <path className="cx-p" d="M894,192 H903" />
                <path className="cx-aht" d="M898,188 L903,192 L898,196 Z" />
                <path className="cx-p" d="M997,192 H1007" />
                <path className="cx-p" d="M1007,192 H1016" />
                <path className="cx-aht" d="M1011,188 L1016,192 L1011,196 Z" />
              </svg>
              <span className="cx-fgt cx-done" style={{ left: "93px", top: "166px" }} title="Artifact: brief"><Ic n="brief" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "93px", top: "198px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "206px", top: "166px" }} title="Artifact: route"><Ic n="gate" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "206px", top: "198px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "319px", top: "166px" }} title="Artifact: diff"><Ic n="diff" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "319px", top: "198px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "432px", top: "166px" }} title="Artifact: checks"><Ic n="check2" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "432px", top: "198px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-live" style={{ left: "545px", top: "166px" }} title="Artifact: findings"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-live" style={{ left: "545px", top: "198px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "658px", top: "166px" }} title="Artifact: findings"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "658px", top: "198px" }} title="Decided by finding"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "771px", top: "166px" }} title="Artifact: findings"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "771px", top: "198px" }} title="Decided by finding"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "884px", top: "166px" }} title="Artifact: merge"><Ic n="branch" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "884px", top: "198px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "997px", top: "166px" }} title="Artifact: review"><Ic n="eye" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "997px", top: "198px" }} title="Decided by you"><Ic n="user" sm /></span>
              <div className="cx-fln" style={{ left: "0px", top: "152px", height: "80px" }}>
                <b>Brief</b>
                <span className="cx-s">Lead</span>
                <span className="cx-s">Opus 5.5</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Written</span>
              </div>
              <div className="cx-fln" style={{ left: "113px", top: "152px", height: "80px" }}>
                <b>Route</b>
                <span className="cx-s">Claude Code</span>
                <span className="cx-s">Sonnet 5.5</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Routed</span>
              </div>
              <div className="cx-fln" style={{ left: "226px", top: "152px", height: "80px" }}>
                <b>Build</b>
                <span className="cx-s">Claude Code</span>
                <span className="cx-s">Sonnet 5.5</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Done</span>
              </div>
              <div className="cx-fln" style={{ left: "339px", top: "152px", height: "80px" }}>
                <b>Checks</b>
                <span className="cx-s">3 of 3</span>
                <span className="cx-s">code</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Passed</span>
              </div>
              <div className="cx-fln" style={{ left: "452px", top: "64px", height: "80px" }}>
                <b>Correctness</b>
                <span className="cx-s">Codex</span>
                <span className="cx-s">GPT-5.5</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />No findings</span>
              </div>
              <div className="cx-fln" style={{ left: "452px", top: "152px", height: "80px" }}>
                <b>Performance</b>
                <span className="cx-s">OpenCode</span>
                <span className="cx-s">DeepSeek V4</span>
                <span className="cx-st cx-info"><i className="cx-spin" aria-label="Working" />Reviewing</span>
              </div>
              <div className="cx-fln" style={{ left: "452px", top: "240px", height: "80px" }}>
                <b>Docs</b>
                <span className="cx-s">OpenCode</span>
                <span className="cx-s">DeepSeek V4</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />1 unconfirmed</span>
              </div>
              <div className="cx-fln cx-csel" style={{ left: "565px", top: "152px", height: "80px" }}>
                <b>Security</b>
                <span className="cx-s">Codex</span>
                <span className="cx-s">GPT-5.5 · Lead</span>
                <span className="cx-st cx-info"><i className="cx-spin" aria-label="Working" />Reviewing</span>
              </div>
              <div className="cx-fln cx-cpend" style={{ left: "678px", top: "152px", height: "80px" }}><b>Finding check</b><span className="cx-s">Code repro</span><span className="cx-st"><i className="cx-sd" />Waiting</span></div>
              <div className="cx-fln cx-cpend" style={{ left: "791px", top: "152px", height: "80px" }}>
                <b>Merge queue</b>
                <span className="cx-s">Re-checks</span>
                <span className="cx-s">combined</span>
                <span className="cx-st"><i className="cx-sd" />Waiting</span>
              </div>
              <div className="cx-fln cx-cpend" style={{ left: "904px", top: "152px", height: "80px" }}><b>Main</b><span className="cx-s">Watched</span><span className="cx-st"><i className="cx-sd" />Waiting</span></div>
              <div className="cx-fln cx-cpend" style={{ left: "1017px", top: "152px", height: "80px" }}>
                <b>You</b>
                <span className="cx-s">Review</span>
                <span className="cx-s">after merge</span>
                <span className="cx-st"><i className="cx-sd" />Waiting</span>
              </div>
            </div>
            <div style={{ padding: "0 20px 6px", fontSize: "12px", flex: "none" }}>
              <div style={{ display: "flex", gap: "8px", alignItems: "center", padding: "5px 0", borderTop: "1px solid var(--color-border)" }}>
                <span className="cx-st cx-info"><i className="cx-spin" aria-label="Working" />Running</span>
                <b style={{ fontWeight: 500, color: "var(--color-text)" }}>Performance</b>
                <span style={{ color: "var(--color-muted)" }}>OpenCode · DeepSeek V4</span>
                <span style={{ color: "var(--color-textSoft)", minWidth: "0", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>Reading auth/redirect.ts and its 3 callers</span>
                <span className="cx-num" style={{ marginLeft: "auto" }}>2m 11s</span>
              </div>
              <div style={{ display: "flex", gap: "8px", alignItems: "center", padding: "5px 0", borderTop: "1px solid var(--color-border)" }}>
                <span className="cx-st cx-info"><i className="cx-spin" aria-label="Working" />Running</span>
                <b style={{ fontWeight: 500, color: "var(--color-text)" }}>Security</b>
                <span style={{ color: "var(--color-muted)" }}>Codex · GPT-5.5 · lead tier</span>
                <span style={{ color: "var(--color-textSoft)", minWidth: "0", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>Tracing the next parameter into the redirect response</span>
                <span className="cx-num" style={{ marginLeft: "auto" }}>1m 40s</span>
              </div>
            </div>
      </div>
      <div className="cx-fdr" style={{ width: "360px" }}>
        <div className="cx-dh">
          <b>Security</b>
          <div style={{ display: "flex", gap: "6px", flexWrap: "wrap", alignItems: "center" }}><span className="cx-st cx-info"><i className="cx-spin" role="img" aria-label="Working" />Reviewing · 1m 40s</span><span className="cx-fbadge cx-sep"><Ic n="shield" sm />Separate run</span><span className="cx-fbadge"><Ic n="eye" sm />Different engine from the author</span></div>
        </div>
        <div className="cx-db">
          <div className="cx-fbx">
            <div className="cx-fkv">
              <span>Engine</span>
              <b>Codex · GPT-5.5 · lead tier</b>
              <span>Checkout</span>
              <b>Read-only at a41c9e2</b>
              <span>Given</span>
              <b>The diff, the brief’s purpose, the check results</b>
              <span>Not given</span>
              <b>The author’s transcript or verdict</b>
            </div>
          </div>
          <div className="cx-fcol">
            <h5>Findings so far</h5>
            <div className="cx-ffd">
              <div className="cx-t"><span className="cx-fsev">medium</span><b>next is accepted when it starts with //</b><span className="cx-ffs cx-u">Unconfirmed</span></div>
              <span className="cx-pa">auth/redirect.ts:42</span>
            </div>
            <p style={{ margin: "0", color: "var(--color-muted)", fontSize: "12px" }}>It blocks if the Finding check reproduces it. Until then it is shown, never counted.</p>
          </div>
        </div>
        <div className="cx-da"><Btn className="cx-btn">Open the run</Btn><Btn className="cx-ghost">Stop this lane</Btn></div>
      </div>
    </Frame>
  );
}
