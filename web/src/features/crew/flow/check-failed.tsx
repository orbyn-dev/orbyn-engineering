import { Frame } from "../shared/Frame";
import { Btn, Seg } from "../shared/parts";
import { FlowHead } from "./FlowHead";
import { Ic } from "./icons";

/** Crew Designs, frame `flow-check-failed`: Flow: a failed check loops back to Build. */
export function FlowCheckFailed() {
  return (
    <Frame height={700}>
      <div className="cx-cwpg">
            <FlowHead sub="Login redirect crew · Task: Use next in the auth redirect · 5 runs" acts />
            <div className="cx-cwtabs">
              <Seg kind="segx" items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} on={0} />
              <span className="cx-end">Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m<span className="cx-cwpv" title="Planned: no Flow route reports usage yet">Planned</span></span>
            </div>
            <div className="cx-flcr">
              <span style={{ color: "var(--color-muted)" }}>All tasks</span>
              <Ic n="right" sm />
              <b>Use next in the auth redirect</b>
              <span className="cx-fbadge">Round 2</span>
              <span className="cx-end"><span className="cx-fil" style={{ display: "inline-flex", alignItems: "center", gap: "6px", height: "26px", padding: "0 10px", borderRadius: "8px", background: "var(--color-sunken)" }}><Ic n="plus" sm />Add reviewer<span className="cx-cwpv">After the beta</span></span><span className="cx-fil" style={{ display: "inline-flex", alignItems: "center", gap: "6px", height: "26px", padding: "0 10px", borderRadius: "8px", background: "var(--color-sunken)" }}><Ic n="down" sm />Round 2</span></span>
            </div>
            <div className="cx-fl" style={{ height: "250px", width: "1110px" }}>
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
              <svg className="cx-fe" width="1110" height="250" aria-hidden="true">
                <path d="M93,120 H103" />
                <path d="M103,120 H112" />
                <path className="cx-aht" d="M107,116 L112,120 L107,124 Z" />
                <path d="M206,120 H216" />
                <path d="M216,120 H225" />
                <path className="cx-aht" d="M220,116 L225,120 L220,124 Z" />
                <path className="cx-l" d="M319,120 H329" />
                <path className="cx-l" d="M329,120 H338" />
                <path className="cx-ahb" d="M333,116 L338,120 L333,124 Z" />
                <path d="M432,120 H442" />
                <path d="M442,120 H451" />
                <path className="cx-aht" d="M446,116 L451,120 L446,124 Z" />
                <path className="cx-p" d="M545,120 H555" />
                <path className="cx-p" d="M555,120 H564" />
                <path className="cx-aht" d="M559,116 L564,120 L559,124 Z" />
                <path className="cx-p" d="M658,120 H668" />
                <path className="cx-p" d="M668,120 H677" />
                <path className="cx-aht" d="M672,116 L677,120 L672,124 Z" />
                <path className="cx-p" d="M771,120 H781" />
                <path className="cx-p" d="M781,120 H790" />
                <path className="cx-aht" d="M785,116 L790,120 L785,124 Z" />
                <path className="cx-p" d="M884,120 H894" />
                <path className="cx-p" d="M894,120 H903" />
                <path className="cx-aht" d="M898,116 L903,120 L898,124 Z" />
                <path className="cx-p" d="M997,120 H1007" />
                <path className="cx-p" d="M1007,120 H1016" />
                <path className="cx-aht" d="M1011,116 L1016,120 L1011,124 Z" />
                <path className="cx-lp" d="M385,160 V188 Q385,194 379,194 H278 Q272,194 272,188 V162" />
                <path className="cx-ah" d="M268,167 L272,161 L276,167 Z" />
              </svg>
              <span className="cx-fgt cx-done" style={{ left: "93px", top: "94px" }} title="Artifact: brief"><Ic n="brief" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "93px", top: "126px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "206px", top: "94px" }} title="Artifact: route"><Ic n="gate" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "206px", top: "126px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-live" style={{ left: "319px", top: "94px" }} title="Artifact: diff"><Ic n="diff" sm /></span>
              <span className="cx-fgt cx-live" style={{ left: "319px", top: "126px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "432px", top: "94px" }} title="Artifact: checks"><Ic n="check2" sm /></span>
              <span className="cx-fgt cx-block" style={{ left: "432px", top: "126px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "545px", top: "94px" }} title="Artifact: findings"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "545px", top: "126px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "658px", top: "94px" }} title="Artifact: findings"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "658px", top: "126px" }} title="Decided by finding"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "771px", top: "94px" }} title="Artifact: findings"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "771px", top: "126px" }} title="Decided by finding"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "884px", top: "94px" }} title="Artifact: merge"><Ic n="branch" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "884px", top: "126px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "997px", top: "94px" }} title="Artifact: review"><Ic n="eye" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "997px", top: "126px" }} title="Decided by you"><Ic n="user" sm /></span>
              <span className="cx-flab cx-r" style={{ left: "328px", top: "184px" }}><b>Check failed</b>{" → retry with the reason · round 2"}</span>
              <div className="cx-fln" style={{ left: "0px", top: "80px", height: "80px" }}>
                <b>Brief</b>
                <span className="cx-s">Lead</span>
                <span className="cx-s">Opus 5.5</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Written</span>
              </div>
              <div className="cx-fln" style={{ left: "113px", top: "80px", height: "80px" }}>
                <b>Route</b>
                <span className="cx-s">Claude Code</span>
                <span className="cx-s">Sonnet 5.5</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Routed</span>
              </div>
              <div className="cx-fln" style={{ left: "226px", top: "80px", height: "80px" }}>
                <b>Build</b>
                <span className="cx-s">Claude Code</span>
                <span className="cx-s">Sonnet 5.5</span>
                <span className="cx-st cx-info"><i className="cx-spin" aria-label="Working" />Fixing</span>
                <span className="cx-rd">Round 2</span>
              </div>
              <div className="cx-fln cx-cfail" style={{ left: "339px", top: "80px", height: "80px" }}>
                <b>Checks</b>
                <span className="cx-s">2 of 3</span>
                <span className="cx-s">code</span>
                <span className="cx-st cx-err"><i className="cx-sd" />Failed</span>
              </div>
              <div className="cx-fln cx-cpend" style={{ left: "452px", top: "80px", height: "80px" }}><b>Code review</b><span className="cx-s">After checks</span><span className="cx-st"><i className="cx-sd" />Waiting</span></div>
              <div className="cx-fln cx-cpend" style={{ left: "565px", top: "80px", height: "80px" }}><b>Security</b><span className="cx-s">After checks</span><span className="cx-st"><i className="cx-sd" />Waiting</span></div>
              <div className="cx-fln cx-cpend" style={{ left: "678px", top: "80px", height: "80px" }}><b>Finding check</b><span className="cx-s">Not started</span><span className="cx-st"><i className="cx-sd" />Waiting</span></div>
              <div className="cx-fln cx-cpend" style={{ left: "791px", top: "80px", height: "80px" }}>
                <b>Merge queue</b>
                <span className="cx-s">Re-checks</span>
                <span className="cx-s">combined</span>
                <span className="cx-st"><i className="cx-sd" />Waiting</span>
              </div>
              <div className="cx-fln cx-cpend" style={{ left: "904px", top: "80px", height: "80px" }}><b>Main</b><span className="cx-s">Watched</span><span className="cx-st"><i className="cx-sd" />Waiting</span></div>
              <div className="cx-fln cx-cpend" style={{ left: "1017px", top: "80px", height: "80px" }}>
                <b>You</b>
                <span className="cx-s">Review</span>
                <span className="cx-s">after merge</span>
                <span className="cx-st"><i className="cx-sd" />Waiting</span>
              </div>
            </div>
            <div style={{ padding: "4px 20px 8px", flex: "none" }}>
              <div className="cx-fbx cx-red" style={{ maxWidth: "640px" }}>
                <h6>Check 3 of 3 failed: Tests (auth)</h6>
                <div className="cx-fkv">
                  <span>Result</span>
                  <b>Failed</b>
                  <span>Sent to Build</span>
                  <b>The failed check’s name and the failing commit, as the retry reason. The command and the output are not given to the worker while the task runs. Nothing is reviewed until the checks pass.</b>
                </div>
              </div>
            </div>
            <div className="cx-flsc">
              <Btn className="cx-ghost" style={{ height: "26px", padding: "0 9px" }} aria-label="Play"><Ic n="play" sm /></Btn>
              <div className="cx-tr">
                <span className="cx-played" style={{ width: "38%" }} />
                <i className="cx-g" style={{ left: "5%" }} />
                <i className="cx-g" style={{ left: "12%" }} />
                <i className="cx-g" style={{ left: "20%" }} />
                <i className="cx-r" style={{ left: "30%" }} />
                <i className="cx-b" style={{ left: "36%" }} />
                <span className="cx-ph2" style={{ left: "38%" }} />
                <span className="cx-tl" style={{ left: "0%" }}>0:00</span>
                <span className="cx-tl" style={{ left: "25%" }}>12:10</span>
                <span className="cx-tl" style={{ left: "50%" }}>24:20</span>
                <span className="cx-tl" style={{ left: "75%" }}>36:30</span>
                <span className="cx-tl" style={{ left: "100%" }}>41:06</span>
              </div>
              <span className="cx-nowc cx-on"><i className="cx-spin" aria-label="Live" />Now · following</span>
            </div>
      </div>
    </Frame>
  );
}
