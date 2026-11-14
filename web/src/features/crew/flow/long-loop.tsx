import { Frame } from "../shared/Frame";
import { PageHead, Seg } from "../shared/parts";
import { Ic } from "./icons";

/** Crew Designs, frame `flow-long-loop`: Flow: a long fix loop (round 7). */
export function FlowLongLoop() {
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
              <span className="cx-fbadge">Round 7</span>
              <span className="cx-end"><span className="cx-fil" style={{ display: "inline-flex", alignItems: "center", gap: "6px", height: "26px", padding: "0 10px", borderRadius: "8px", background: "var(--color-sunken)" }}><Ic n="plus" sm />Add reviewer<span className="cx-cwpv">After the beta</span></span><span className="cx-fil" style={{ display: "inline-flex", alignItems: "center", gap: "6px", height: "26px", padding: "0 10px", borderRadius: "8px", background: "var(--color-sunken)" }}><Ic n="down" sm />Round 1</span></span>
            </div>
            <div className="cx-fhist" style={{ padding: "8px 20px", flex: "none" }}>
              <span style={{ color: "var(--color-muted)" }}>Rounds</span>
              <span className="cx-rd2">1 to 5 · 4 findings fixed ▸</span>
              <span className="cx-rd2 cx-bad">6 · 1 regressed</span>
              <span className="cx-rd2 cx-cur"><i className="cx-spin" aria-label="Running" />7 · running</span>
              <span style={{ marginLeft: "auto", color: "var(--color-muted)" }}>Click a round to see its graph</span>
            </div>
            <div className="cx-fl" style={{ height: "262px", width: "1110px" }}>
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
              <svg className="cx-fe" width="1110" height="262" aria-hidden="true">
                <path d="M93,156 H103" />
                <path d="M103,156 H112" />
                <path className="cx-aht" d="M107,152 L112,156 L107,160 Z" />
                <path d="M206,156 H216" />
                <path d="M216,156 H225" />
                <path className="cx-aht" d="M220,152 L225,156 L220,160 Z" />
                <path d="M319,156 H329" />
                <path d="M329,156 H338" />
                <path className="cx-aht" d="M333,152 L338,156 L333,160 Z" />
                <path d="M432,156 H442" />
                <path d="M442,112 H451" />
                <path className="cx-aht" d="M446,108 L451,112 L446,116 Z" />
                <path d="M442,200 H451" />
                <path className="cx-aht" d="M446,196 L451,200 L446,204 Z" />
                <path d="M442,112 V200" />
                <path className="cx-l" d="M545,112 H555" />
                <path className="cx-l" d="M545,200 H555" />
                <path className="cx-l" d="M555,156 H564" />
                <path className="cx-ahb" d="M559,152 L564,156 L559,160 Z" />
                <path className="cx-l" d="M555,112 V200" />
                <path className="cx-p" d="M658,156 H668" />
                <path className="cx-p" d="M668,156 H677" />
                <path className="cx-aht" d="M672,152 L677,156 L672,160 Z" />
                <path d="M771,156 H781" />
                <path d="M781,156 H790" />
                <path className="cx-aht" d="M785,152 L790,156 L785,160 Z" />
                <path className="cx-p" d="M884,156 H894" />
                <path className="cx-p" d="M894,156 H903" />
                <path className="cx-aht" d="M898,152 L903,156 L898,160 Z" />
                <path className="cx-p" d="M997,156 H1007" />
                <path className="cx-p" d="M1007,156 H1016" />
                <path className="cx-aht" d="M1011,152 L1016,156 L1011,160 Z" />
                <path className="cx-lp" d="M724,196 V260 Q724,266 718,266 H278 Q272,266 272,260 V198" />
                <path className="cx-ah" d="M268,203 L272,197 L276,203 Z" />
              </svg>
              <span className="cx-fgt cx-done" style={{ left: "93px", top: "130px" }} title="Artifact: brief"><Ic n="brief" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "93px", top: "162px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "206px", top: "130px" }} title="Artifact: route"><Ic n="gate" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "206px", top: "162px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "319px", top: "130px" }} title="Artifact: diff"><Ic n="diff" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "319px", top: "162px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "432px", top: "130px" }} title="Artifact: checks"><Ic n="check2" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "432px", top: "162px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-live" style={{ left: "545px", top: "130px" }} title="Artifact: findings"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-live" style={{ left: "545px", top: "162px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "658px", top: "130px" }} title="Artifact: findings"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "658px", top: "162px" }} title="Decided by finding"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "771px", top: "130px" }} title="Artifact: findings"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-block" style={{ left: "771px", top: "162px" }} title="Decided by finding"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "884px", top: "130px" }} title="Artifact: merge"><Ic n="branch" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "884px", top: "162px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "997px", top: "130px" }} title="Artifact: review"><Ic n="eye" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "997px", top: "162px" }} title="Decided by you"><Ic n="user" sm /></span>
              <span className="cx-flab cx-r" style={{ left: "498px", top: "256px" }}><b>Round 7</b>{" · fix brief from F5 and F6"}</span>
              <div className="cx-fln" style={{ left: "0px", top: "116px", height: "80px" }}>
                <b>Brief</b>
                <span className="cx-s">Lead</span>
                <span className="cx-s">Opus 5.5</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Written</span>
              </div>
              <div className="cx-fln" style={{ left: "113px", top: "116px", height: "80px" }}>
                <b>Route</b>
                <span className="cx-s">Claude Code</span>
                <span className="cx-s">Sonnet 5.5</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Routed</span>
              </div>
              <div className="cx-fln" style={{ left: "226px", top: "116px", height: "80px" }}>
                <b>Build</b>
                <span className="cx-s">Claude Code</span>
                <span className="cx-s">Sonnet 5.5</span>
                <span className="cx-st cx-info"><i className="cx-spin" aria-label="Working" />Fixing</span>
                <span className="cx-rd">Round 7</span>
              </div>
              <div className="cx-fln cx-cpend" style={{ left: "339px", top: "116px", height: "80px" }}>
                <b>Checks</b>
                <span className="cx-s">Waiting</span>
                <span className="cx-s">round 7</span>
                <span className="cx-st"><i className="cx-sd" />Waiting</span>
              </div>
              <div className="cx-fln cx-cfail cx-cdim" style={{ left: "452px", top: "72px", height: "80px" }}>
                <b>Correctness</b>
                <span className="cx-s">Codex</span>
                <span className="cx-s">GPT-5.5</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />1 open</span>
              </div>
              <div className="cx-fln cx-cdim" style={{ left: "452px", top: "160px", height: "80px" }}>
                <b>Performance</b>
                <span className="cx-s">OpenCode</span>
                <span className="cx-s">DeepSeek V4</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />No findings</span>
              </div>
              <div className="cx-fln cx-cfail cx-cdim" style={{ left: "565px", top: "116px", height: "80px" }}>
                <b>Security</b>
                <span className="cx-s">Codex</span>
                <span className="cx-s">GPT-5.5 · Lead</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />1 regressed</span>
              </div>
              <div className="cx-fln cx-cfail cx-cdim" style={{ left: "678px", top: "116px", height: "80px" }}><b>Finding check</b><span className="cx-s">Code repro</span><span className="cx-st cx-err"><i className="cx-sd" />2 confirmed</span></div>
              <div className="cx-fln cx-cpend" style={{ left: "791px", top: "116px", height: "80px" }}>
                <b>Merge queue</b>
                <span className="cx-s">Re-checks</span>
                <span className="cx-s">combined</span>
                <span className="cx-st"><i className="cx-sd" />Waiting</span>
              </div>
              <div className="cx-fln cx-cpend" style={{ left: "904px", top: "116px", height: "80px" }}><b>Main</b><span className="cx-s">Watched</span><span className="cx-st"><i className="cx-sd" />Waiting</span></div>
              <div className="cx-fln cx-cpend" style={{ left: "1017px", top: "116px", height: "80px" }}>
                <b>You</b>
                <span className="cx-s">Review</span>
                <span className="cx-s">after merge</span>
                <span className="cx-st"><i className="cx-sd" />Waiting</span>
              </div>
            </div>
            <div className="cx-fta" style={{ padding: "0 20px" }}>
              <div className="cx-r cx-h" style={{ gridTemplateColumns: "48px minmax(0,1fr) 190px 150px 110px", minHeight: "24px" }}>
                <span>Id</span>
                <span>Finding</span>
                <span>Where</span>
                <span>History</span>
                <span>Now</span>
              </div>
              <div className="cx-r" style={{ gridTemplateColumns: "48px minmax(0,1fr) 190px 150px 110px", minHeight: "28px", padding: "3px 0" }}>
                <span className="cx-num">F1–F4</span>
                <b style={{ fontWeight: 400 }}>4 earlier findings, each confirmed and sent back once</b>
                <span className="cx-pa" style={{ fontFamily: "var(--font-mono)", fontSize: "11px", color: "var(--color-muted)" }}>auth/redirect.ts and 1 more</span>
                <span>found r1–r4 · fixed r2–r5</span>
                <span className="cx-ffs cx-f">Fixed</span>
              </div>
              <div className="cx-r" style={{ gridTemplateColumns: "48px minmax(0,1fr) 190px 150px 110px", minHeight: "28px", padding: "3px 0" }}>
                <span className="cx-num">F5</span>
                <b style={{ fontWeight: 400 }}>next survives a failed login</b>
                <span className="cx-pa" style={{ fontFamily: "var(--font-mono)", fontSize: "11px", color: "var(--color-muted)" }}>auth/login.ts:71</span>
                <span>found r5 · open in r6, r7</span>
                <span className="cx-ffs cx-o">Still open</span>
              </div>
              <div className="cx-r" style={{ gridTemplateColumns: "48px minmax(0,1fr) 190px 150px 110px", minHeight: "28px", padding: "3px 0" }}>
                <span className="cx-num">F6</span>
                <b style={{ fontWeight: 400 }}>F1 returned after the refactor</b>
                <span className="cx-pa" style={{ fontFamily: "var(--font-mono)", fontSize: "11px", color: "var(--color-muted)" }}>auth/redirect.ts:42</span>
                <span>fixed r2 · back in r6</span>
                <span className="cx-ffs cx-rg">Regressed</span>
              </div>
            </div>
      </div>
    </Frame>
  );
}
