import { Frame } from "../shared/Frame";
import { Btn, PageHead, Seg } from "../shared/parts";
import { Ic } from "./icons";

/** Crew Designs, frame `flow-pr-review`: Flow: PR review after the PR opens. */
export function FlowPrReview() {
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
              <span className="cx-fbadge">PR #482</span>
              <span className="cx-end"><span className="cx-fil" style={{ display: "inline-flex", alignItems: "center", gap: "6px", height: "26px", padding: "0 10px", borderRadius: "8px", background: "var(--color-sunken)" }}><Ic n="plus" sm />Add reviewer</span><span className="cx-fil" style={{ display: "inline-flex", alignItems: "center", gap: "6px", height: "26px", padding: "0 10px", borderRadius: "8px", background: "var(--color-sunken)" }}><Ic n="down" sm />Round 1</span></span>
            </div>
            <div className="cx-fl" style={{ height: "300px", width: "1110px" }}>
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
              <svg className="cx-fe" width="1110" height="300" aria-hidden="true">
                <path d="M93,170 H103" />
                <path d="M103,170 H112" />
                <path className="cx-aht" d="M107,166 L112,170 L107,174 Z" />
                <path d="M206,170 H216" />
                <path d="M216,170 H225" />
                <path className="cx-aht" d="M220,166 L225,170 L220,174 Z" />
                <path d="M319,170 H329" />
                <path d="M329,170 H338" />
                <path className="cx-aht" d="M333,166 L338,170 L333,174 Z" />
                <path d="M432,170 H442" />
                <path d="M442,113 H451" />
                <path className="cx-aht" d="M446,109 L451,113 L446,117 Z" />
                <path d="M442,151 H451" />
                <path className="cx-aht" d="M446,147 L451,151 L446,155 Z" />
                <path d="M442,189 H451" />
                <path className="cx-aht" d="M446,185 L451,189 L446,193 Z" />
                <path d="M442,227 H451" />
                <path className="cx-aht" d="M446,223 L451,227 L446,231 Z" />
                <path d="M442,113 V227" />
                <path d="M545,113 H555" />
                <path d="M545,151 H555" />
                <path d="M545,189 H555" />
                <path d="M545,227 H555" />
                <path d="M555,170 H564" />
                <path className="cx-aht" d="M559,166 L564,170 L559,174 Z" />
                <path d="M555,113 V227" />
                <path d="M658,170 H668" />
                <path d="M668,170 H677" />
                <path className="cx-aht" d="M672,166 L677,170 L672,174 Z" />
                <path className="cx-l" d="M771,170 H781" />
                <path className="cx-l" d="M781,170 H790" />
                <path className="cx-ahb" d="M785,166 L790,170 L785,174 Z" />
                <path className="cx-p" d="M884,170 H894" />
                <path className="cx-p" d="M894,170 H903" />
                <path className="cx-aht" d="M898,166 L903,170 L898,174 Z" />
                <path className="cx-p" d="M997,170 H1007" />
                <path className="cx-p" d="M1007,170 H1016" />
                <path className="cx-aht" d="M1011,166 L1016,170 L1011,174 Z" />
                <path className="cx-pr" d="M498,242 V270 Q498,276 492,276 H278 Q272,276 272,270 V212" />
                <path className="cx-ah" d="M268,217 L272,211 L276,217 Z" />
              </svg>
              <span className="cx-fgt cx-done" style={{ left: "93px", top: "144px" }} title="Artifact: brief"><Ic n="brief" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "93px", top: "176px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "206px", top: "144px" }} title="Artifact: route"><Ic n="gate" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "206px", top: "176px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "319px", top: "144px" }} title="Artifact: diff"><Ic n="diff" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "319px", top: "176px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "432px", top: "144px" }} title="Artifact: checks"><Ic n="check2" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "432px", top: "176px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "545px", top: "144px" }} title="Artifact: findings"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "545px", top: "176px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "658px", top: "144px" }} title="Artifact: findings"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "658px", top: "176px" }} title="Decided by finding"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-live" style={{ left: "771px", top: "144px" }} title="Artifact: pr"><Ic n="pr" sm /></span>
              <span className="cx-fgt cx-live" style={{ left: "771px", top: "176px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "884px", top: "144px" }} title="Artifact: merge"><Ic n="branch" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "884px", top: "176px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "997px", top: "144px" }} title="Artifact: review"><Ic n="eye" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "997px", top: "176px" }} title="Decided by you"><Ic n="user" sm /></span>
              <span className="cx-flab" style={{ left: "385px", top: "266px" }}><b>PR findings</b>{" → back to Build when confirmed"}</span>
              <div className="cx-fln" style={{ left: "0px", top: "130px", height: "80px" }}>
                <b>Brief</b>
                <span className="cx-s">Lead</span>
                <span className="cx-s">Opus 5.5</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Written</span>
              </div>
              <div className="cx-fln" style={{ left: "113px", top: "130px", height: "80px" }}>
                <b>Route</b>
                <span className="cx-s">Claude Code</span>
                <span className="cx-s">Sonnet 5.5</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Routed</span>
              </div>
              <div className="cx-fln" style={{ left: "226px", top: "130px", height: "80px" }}>
                <b>Build</b>
                <span className="cx-s">Claude Code</span>
                <span className="cx-s">Sonnet 5.5</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Done</span>
              </div>
              <div className="cx-fln" style={{ left: "339px", top: "130px", height: "80px" }}>
                <b>Checks</b>
                <span className="cx-s">3 of 3</span>
                <span className="cx-s">code</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Passed</span>
              </div>
              <div className="cx-fln cx-cc" style={{ left: "452px", top: "98px", height: "30px" }} title="Correctness"><b>Correctness</b><span className="cx-gl"><Ic n="ok" sm /></span></div>
              <div className="cx-fln cx-cc" style={{ left: "452px", top: "136px", height: "30px" }} title="Performance"><b>Performance</b><span className="cx-gl"><Ic n="ok" sm /></span></div>
              <div className="cx-fln cx-cc cx-cpr" style={{ left: "452px", top: "174px", height: "30px" }} title="Correctness"><b>Correctness</b><span className="cx-gl"><i className="cx-spin" aria-label="Working" /></span></div>
              <div className="cx-fln cx-cc cx-cpr" style={{ left: "452px", top: "212px", height: "30px" }} title="Security"><b>Security</b><span className="cx-gl"><Ic n="ok" sm /></span></div>
              <div className="cx-fln" style={{ left: "565px", top: "130px", height: "80px" }}>
                <b>Security</b>
                <span className="cx-s">Codex</span>
                <span className="cx-s">GPT-5.5 · Lead</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />No findings</span>
              </div>
              <div className="cx-fln" style={{ left: "678px", top: "130px", height: "80px" }}><b>Finding check</b><span className="cx-s">Code repro</span><span className="cx-st cx-ok"><i className="cx-sd" />Nothing</span></div>
              <div className="cx-fln cx-cpend" style={{ left: "791px", top: "130px", height: "80px" }}>
                <b>Merge queue</b>
                <span className="cx-s">Re-checks</span>
                <span className="cx-s">combined</span>
                <span className="cx-st"><i className="cx-sd" />Waiting</span>
              </div>
              <div className="cx-fln cx-cpend" style={{ left: "904px", top: "130px", height: "80px" }}><b>Main</b><span className="cx-s">Watched</span><span className="cx-st"><i className="cx-sd" />Waiting</span></div>
              <div className="cx-fln cx-cpend" style={{ left: "1017px", top: "130px", height: "80px" }}>
                <b>You</b>
                <span className="cx-s">Review</span>
                <span className="cx-s">after merge</span>
                <span className="cx-st"><i className="cx-sd" />Waiting</span>
              </div>
              <span className="cx-flab" style={{ left: "498px", top: "62px" }}>Dashed lanes: on PR #482</span>
            </div>
            <div className="cx-flk">
              <span>On the lines:</span>
              <span><i className="cx-fgt cx-done"><Ic n="brief" sm /></i>brief</span>
              <span><i className="cx-fgt cx-done"><Ic n="diff" sm /></i>diff</span>
              <span><i className="cx-fgt cx-done"><Ic n="check2" sm /></i>check results</span>
              <span><i className="cx-fgt cx-done"><Ic n="bug" sm /></i>findings</span>
              <span><i className="cx-fgt cx-done"><Ic n="pr" sm /></i>PR</span>
              <span style={{ marginLeft: "8px" }}>Gate decided by:</span>
              <span><i className="cx-fgt cx-done"><Ic n="cpu" sm /></i>code</span>
              <span><i className="cx-fgt cx-done"><Ic n="bug" sm /></i>a confirmed finding</span>
              <span><i className="cx-fgt cx-done"><Ic n="user" sm /></i>you</span>
              <span>Click a line to open what it carries</span>
            </div>
            <div className="cx-flsc">
              <Btn className="cx-ghost" style={{ height: "26px", padding: "0 9px" }} aria-label="Play"><Ic n="play" sm /></Btn>
              <div className="cx-tr">
                <span className="cx-played" style={{ width: "94%" }} />
                <i className="cx-g" style={{ left: "5%" }} />
                <i className="cx-g" style={{ left: "14%" }} />
                <i className="cx-g" style={{ left: "22%" }} />
                <i className="cx-g" style={{ left: "34%" }} />
                <i className="cx-g" style={{ left: "46%" }} />
                <i className="cx-g" style={{ left: "60%" }} />
                <i className="cx-g" style={{ left: "72%" }} />
                <i className="cx-g" style={{ left: "84%" }} />
                <i className="cx-b" style={{ left: "92%" }} />
                <span className="cx-ph2" style={{ left: "94%" }} />
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
