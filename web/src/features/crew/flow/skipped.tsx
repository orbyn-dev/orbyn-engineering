import { Frame } from "../shared/Frame";
import { Btn, Seg } from "../shared/parts";
import { FlowHead } from "./FlowHead";
import { Ic } from "./icons";

/** Crew Designs, frame `flow-skipped`: Flow: skipped stages. */
export function FlowSkipped() {
  return (
    <Frame height={700}>
      <div className="cx-cwpg">
            <FlowHead sub="Login redirect crew · Task: Update the login docs · 5 runs" acts />
            <div className="cx-cwtabs">
              <Seg kind="segx" items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} on={0} />
              <span className="cx-end">Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m<span className="cx-cwpv" title="Planned: no Flow route reports usage yet">Planned</span></span>
            </div>
            <div className="cx-flcr">
              <span style={{ color: "var(--color-muted)" }}>All tasks</span>
              <Ic n="right" sm />
              <b>Update the login docs</b>
              <span className="cx-end"><span className="cx-fil" style={{ display: "inline-flex", alignItems: "center", gap: "6px", height: "26px", padding: "0 10px", borderRadius: "8px", background: "var(--color-sunken)" }}><Ic n="plus" sm />Add reviewer<span className="cx-cwpv">After the beta</span></span><span className="cx-fil" style={{ display: "inline-flex", alignItems: "center", gap: "6px", height: "26px", padding: "0 10px", borderRadius: "8px", background: "var(--color-sunken)" }}><Ic n="down" sm />Round 1</span></span>
            </div>
            <div className="cx-fl" style={{ height: "270px", width: "1110px" }}>
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
              <svg className="cx-fe" width="1110" height="270" aria-hidden="true">
                <path d="M93,150 H103" />
                <path d="M103,150 H112" />
                <path className="cx-aht" d="M107,146 L112,150 L107,154 Z" />
                <path d="M206,150 H216" />
                <path d="M216,150 H225" />
                <path className="cx-aht" d="M220,146 L225,150 L220,154 Z" />
                <path d="M319,150 H329" />
                <path d="M329,150 H338" />
                <path className="cx-aht" d="M333,146 L338,150 L333,154 Z" />
                <path d="M432,150 H442" />
                <path d="M442,106 H451" />
                <path className="cx-aht" d="M446,102 L451,106 L446,110 Z" />
                <path d="M442,194 H451" />
                <path className="cx-aht" d="M446,190 L451,194 L446,198 Z" />
                <path d="M442,106 V194" />
                <path d="M545,106 H555" />
                <path d="M545,194 H555" />
                <path d="M555,150 H564" />
                <path className="cx-aht" d="M559,146 L564,150 L559,154 Z" />
                <path d="M555,106 V194" />
                <path className="cx-x" d="M658,150 H668" />
                <path className="cx-x" d="M668,150 H677" />
                <path className="cx-aht" d="M672,146 L677,150 L672,154 Z" />
                <path className="cx-x" d="M771,150 H781" />
                <path className="cx-x" d="M781,150 H790" />
                <path className="cx-aht" d="M785,146 L790,150 L785,154 Z" />
                <path className="cx-p" d="M884,150 H894" />
                <path className="cx-p" d="M894,150 H903" />
                <path className="cx-aht" d="M898,146 L903,150 L898,154 Z" />
                <path className="cx-p" d="M997,150 H1007" />
                <path className="cx-p" d="M1007,150 H1016" />
                <path className="cx-aht" d="M1011,146 L1016,150 L1011,154 Z" />
              </svg>
              <span className="cx-fgt cx-done" style={{ left: "93px", top: "124px" }} title="Artifact: brief"><Ic n="brief" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "93px", top: "156px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "206px", top: "124px" }} title="Artifact: route"><Ic n="gate" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "206px", top: "156px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "319px", top: "124px" }} title="Artifact: diff"><Ic n="diff" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "319px", top: "156px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "432px", top: "124px" }} title="Artifact: checks"><Ic n="check2" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "432px", top: "156px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "545px", top: "124px" }} title="Artifact: findings"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "545px", top: "156px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-skip" style={{ left: "658px", top: "124px" }} title="Artifact: findings"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-skip" style={{ left: "658px", top: "156px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-skip" style={{ left: "771px", top: "124px" }} title="Artifact: findings"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-skip" style={{ left: "771px", top: "156px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "884px", top: "124px" }} title="Artifact: merge"><Ic n="branch" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "884px", top: "156px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "997px", top: "124px" }} title="Artifact: review"><Ic n="eye" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "997px", top: "156px" }} title="Decided by you"><Ic n="user" sm /></span>
              <div className="cx-fln" style={{ left: "0px", top: "110px", height: "80px" }}>
                <b>Brief</b>
                <span className="cx-s">Lead</span>
                <span className="cx-s">Opus 5.5</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Written</span>
              </div>
              <div className="cx-fln" style={{ left: "113px", top: "110px", height: "80px" }}>
                <b>Route</b>
                <span className="cx-s">Claude Code</span>
                <span className="cx-s">Sonnet 5.5</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Routed</span>
              </div>
              <div className="cx-fln" style={{ left: "226px", top: "110px", height: "80px" }}>
                <b>Build</b>
                <span className="cx-s">Claude Code</span>
                <span className="cx-s">Sonnet 5.5</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Done</span>
              </div>
              <div className="cx-fln" style={{ left: "339px", top: "110px", height: "80px" }}>
                <b>Checks</b>
                <span className="cx-s">2 of 2</span>
                <span className="cx-s">code</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Passed</span>
              </div>
              <div className="cx-fln" style={{ left: "452px", top: "66px", height: "80px" }}>
                <b>Docs</b>
                <span className="cx-s">OpenCode</span>
                <span className="cx-s">DeepSeek V4</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />No findings</span>
              </div>
              <div className="cx-fln" style={{ left: "452px", top: "154px", height: "80px" }}>
                <b>Licence</b>
                <span className="cx-s">OpenCode</span>
                <span className="cx-s">DeepSeek V4</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />No findings</span>
              </div>
              <div className="cx-fln cx-cskip" style={{ left: "565px", top: "110px", height: "80px" }}><b>Security</b><span className="cx-s">Docs only</span><span className="cx-st">Skipped</span></div>
              <div className="cx-fln cx-cskip" style={{ left: "678px", top: "110px", height: "80px" }}><b>Finding check</b><span className="cx-s">No findings</span><span className="cx-st">Skipped</span></div>
              <div className="cx-fln cx-cpend" style={{ left: "791px", top: "110px", height: "80px" }}>
                <b>Merge queue</b>
                <span className="cx-s">Re-checks</span>
                <span className="cx-s">combined</span>
                <span className="cx-st"><i className="cx-sd" />Waiting</span>
              </div>
              <div className="cx-fln cx-cpend" style={{ left: "904px", top: "110px", height: "80px" }}><b>Main</b><span className="cx-s">Watched</span><span className="cx-st"><i className="cx-sd" />Waiting</span></div>
              <div className="cx-fln" style={{ left: "1017px", top: "110px", height: "80px" }}>
                <b>You</b>
                <span className="cx-s">Review</span>
                <span className="cx-s">after merge</span>
                <span className="cx-st cx-warn"><i className="cx-sd" />Needs you</span>
              </div>
            </div>
            <div className="cx-fta" style={{ padding: "0 20px", marginTop: "8px" }}>
              <div className="cx-r cx-h" style={{ gridTemplateColumns: "130px minmax(0,1fr) 230px 120px", minHeight: "24px" }}>
                <span>Skipped stage</span>
                <span>Why</span>
                <span>Rule</span>
                <span />
              </div>
              <div className="cx-r" style={{ gridTemplateColumns: "130px minmax(0,1fr) 230px 120px", minHeight: "34px" }}>
                <b>Security review</b>
                <span>skipped: docs only (no auth, key, network or dependency files changed)</span>
                <span className="cx-num">built in · for docs-only diffs</span>
                <Btn className="cx-ghost">Run it anyway</Btn>
              </div>
              <div className="cx-r" style={{ gridTemplateColumns: "130px minmax(0,1fr) 230px 120px", minHeight: "34px" }}>
                <b>Finding check</b>
                <span>Nothing to check: no review reported a finding</span>
                <span className="cx-num">built in</span>
                <Btn className="cx-ghost">Run it anyway</Btn>
              </div>
              <div className="cx-r" style={{ gridTemplateColumns: "130px minmax(0,1fr) 230px 120px", minHeight: "34px" }}>
                <b>PR review</b>
                <span>Optional by default<span className="cx-cwpv">Planned</span></span>
                <span className="cx-num">built in · optional</span>
                <Btn className="cx-ghost">Run it anyway</Btn>
              </div>
            </div>
      </div>
    </Frame>
  );
}
