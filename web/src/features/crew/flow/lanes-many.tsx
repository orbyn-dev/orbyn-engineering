import { Frame } from "../shared/Frame";
import { Btn, PageHead, Seg } from "../shared/parts";
import { Ic } from "./icons";

/** Crew Designs, frame `flow-lanes-many`: Flow: 11 reviewer lanes. */
export function FlowLanesMany() {
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
              <span className="cx-fbadge">11 lanes</span>
              <span className="cx-end"><span className="cx-fil" style={{ display: "inline-flex", alignItems: "center", gap: "6px", height: "26px", padding: "0 10px", borderRadius: "8px", background: "var(--color-sunken)" }}><Ic n="plus" sm />Add reviewer</span><span className="cx-fil" style={{ display: "inline-flex", alignItems: "center", gap: "6px", height: "26px", padding: "0 10px", borderRadius: "8px", background: "var(--color-sunken)" }}><Ic n="down" sm />Round 1</span></span>
            </div>
            <div className="cx-fl" style={{ height: "350px", width: "1110px" }}>
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
              <svg className="cx-fe" width="1110" height="350" aria-hidden="true">
                <path d="M93,208 H103" />
                <path d="M103,208 H112" />
                <path className="cx-aht" d="M107,204 L112,208 L107,212 Z" />
                <path d="M206,208 H216" />
                <path d="M216,208 H225" />
                <path className="cx-aht" d="M220,204 L225,208 L220,212 Z" />
                <path d="M319,208 H329" />
                <path d="M329,208 H338" />
                <path className="cx-aht" d="M333,204 L338,208 L333,212 Z" />
                <path d="M432,208 H442" />
                <path d="M442,75 H451" />
                <path className="cx-aht" d="M446,71 L451,75 L446,79 Z" />
                <path d="M442,113 H451" />
                <path className="cx-aht" d="M446,109 L451,113 L446,117 Z" />
                <path d="M442,151 H451" />
                <path className="cx-aht" d="M446,147 L451,151 L446,155 Z" />
                <path d="M442,189 H451" />
                <path className="cx-aht" d="M446,185 L451,189 L446,193 Z" />
                <path d="M442,227 H451" />
                <path className="cx-aht" d="M446,223 L451,227 L446,231 Z" />
                <path d="M442,265 H451" />
                <path className="cx-aht" d="M446,261 L451,265 L446,269 Z" />
                <path d="M442,303 H451" />
                <path className="cx-aht" d="M446,299 L451,303 L446,307 Z" />
                <path d="M442,341 H451" />
                <path className="cx-aht" d="M446,337 L451,341 L446,345 Z" />
                <path d="M442,75 V341" />
                <path className="cx-l" d="M545,75 H555" />
                <path className="cx-l" d="M545,113 H555" />
                <path className="cx-l" d="M545,151 H555" />
                <path className="cx-l" d="M545,189 H555" />
                <path className="cx-l" d="M545,227 H555" />
                <path className="cx-l" d="M545,265 H555" />
                <path className="cx-l" d="M545,303 H555" />
                <path className="cx-l" d="M545,341 H555" />
                <path className="cx-l" d="M555,170 H564" />
                <path className="cx-ahb" d="M559,166 L564,170 L559,174 Z" />
                <path className="cx-l" d="M555,208 H564" />
                <path className="cx-ahb" d="M559,204 L564,208 L559,212 Z" />
                <path className="cx-l" d="M555,246 H564" />
                <path className="cx-ahb" d="M559,242 L564,246 L559,250 Z" />
                <path className="cx-l" d="M555,75 V341" />
                <path className="cx-p" d="M658,170 H668" />
                <path className="cx-p" d="M658,208 H668" />
                <path className="cx-p" d="M658,246 H668" />
                <path className="cx-p" d="M668,208 H677" />
                <path className="cx-aht" d="M672,204 L677,208 L672,212 Z" />
                <path className="cx-p" d="M668,170 V246" />
                <path className="cx-p" d="M771,208 H781" />
                <path className="cx-p" d="M781,208 H790" />
                <path className="cx-aht" d="M785,204 L790,208 L785,212 Z" />
                <path className="cx-p" d="M884,208 H894" />
                <path className="cx-p" d="M894,208 H903" />
                <path className="cx-aht" d="M898,204 L903,208 L898,212 Z" />
                <path className="cx-p" d="M997,208 H1007" />
                <path className="cx-p" d="M1007,208 H1016" />
                <path className="cx-aht" d="M1011,204 L1016,208 L1011,212 Z" />
              </svg>
              <span className="cx-fgt cx-done" style={{ left: "93px", top: "182px" }} title="Artifact: brief"><Ic n="brief" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "93px", top: "214px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "206px", top: "182px" }} title="Artifact: route"><Ic n="gate" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "206px", top: "214px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "319px", top: "182px" }} title="Artifact: diff"><Ic n="diff" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "319px", top: "214px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "432px", top: "182px" }} title="Artifact: checks"><Ic n="check2" sm /></span>
              <span className="cx-fgt cx-done" style={{ left: "432px", top: "214px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-live" style={{ left: "545px", top: "182px" }} title="Artifact: findings"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-live" style={{ left: "545px", top: "214px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "658px", top: "182px" }} title="Artifact: findings"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "658px", top: "214px" }} title="Decided by finding"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "771px", top: "182px" }} title="Artifact: findings"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "771px", top: "214px" }} title="Decided by finding"><Ic n="bug" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "884px", top: "182px" }} title="Artifact: merge"><Ic n="branch" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "884px", top: "214px" }} title="Decided by code"><Ic n="cpu" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "997px", top: "182px" }} title="Artifact: review"><Ic n="eye" sm /></span>
              <span className="cx-fgt cx-pend" style={{ left: "997px", top: "214px" }} title="Decided by you"><Ic n="user" sm /></span>
              <div className="cx-fln" style={{ left: "0px", top: "168px", height: "80px" }}>
                <b>Brief</b>
                <span className="cx-s">Lead</span>
                <span className="cx-s">Opus 5.5</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Written</span>
              </div>
              <div className="cx-fln" style={{ left: "113px", top: "168px", height: "80px" }}>
                <b>Route</b>
                <span className="cx-s">Claude Code</span>
                <span className="cx-s">Sonnet 5.5</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Routed</span>
              </div>
              <div className="cx-fln" style={{ left: "226px", top: "168px", height: "80px" }}>
                <b>Build</b>
                <span className="cx-s">Claude Code</span>
                <span className="cx-s">Sonnet 5.5</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Done</span>
              </div>
              <div className="cx-fln" style={{ left: "339px", top: "168px", height: "80px" }}>
                <b>Checks</b>
                <span className="cx-s">3 of 3</span>
                <span className="cx-s">code</span>
                <span className="cx-st cx-ok"><i className="cx-sd" />Passed</span>
              </div>
              <div className="cx-fln cx-cc" style={{ left: "452px", top: "60px", height: "30px" }} title="Correctness"><b>Correctness</b><span className="cx-gl"><Ic n="ok" sm /></span></div>
              <div className="cx-fln cx-cc" style={{ left: "452px", top: "98px", height: "30px" }} title="Performance"><b>Performance</b><span className="cx-gl"><Ic n="ok" sm /></span></div>
              <div className="cx-fln cx-cc" style={{ left: "452px", top: "136px", height: "30px" }} title="Accessibility"><b>Accessibility</b><span className="cx-gl"><i className="cx-spin" aria-label="Working" /></span></div>
              <div className="cx-fln cx-cc" style={{ left: "452px", top: "174px", height: "30px" }} title="Design"><b>Design</b><span className="cx-gl"><Ic n="ok" sm /></span></div>
              <div className="cx-fln cx-cc" style={{ left: "452px", top: "212px", height: "30px" }} title="Licence"><b>Licence</b><span className="cx-gl"><Ic n="ok" sm /></span></div>
              <div className="cx-fln cx-cc" style={{ left: "452px", top: "250px", height: "30px" }} title="Docs"><b>Docs</b><span className="cx-gl"><Ic n="ok" sm /></span></div>
              <div className="cx-fln cx-cc" style={{ left: "452px", top: "288px", height: "30px" }} title="Types"><b>Types</b><span className="cx-gl"><i className="cx-spin" aria-label="Working" /></span></div>
              <div className="cx-fln cx-cc" style={{ left: "452px", top: "326px", height: "30px" }} title="Telemetry"><b>Telemetry</b><span className="cx-gl"><Ic n="clock" sm /></span></div>
              <div className="cx-fln cx-cc" style={{ left: "565px", top: "155px", height: "30px" }} title="Security"><b>Security</b><span className="cx-gl"><i className="cx-spin" aria-label="Working" /></span></div>
              <div className="cx-fln cx-cc" style={{ left: "565px", top: "193px", height: "30px" }} title="Secrets"><b>Secrets</b><span className="cx-gl"><Ic n="ok" sm /></span></div>
              <div className="cx-fln cx-cc" style={{ left: "565px", top: "231px", height: "30px" }} title="Keys"><b>Keys</b><span className="cx-gl"><Ic n="ok" sm /></span></div>
              <div className="cx-fln cx-cpend" style={{ left: "678px", top: "168px", height: "80px" }}><b>Finding check</b><span className="cx-s">Code repro</span><span className="cx-st"><i className="cx-sd" />Waiting</span></div>
              <div className="cx-fln cx-cpend" style={{ left: "791px", top: "168px", height: "80px" }}>
                <b>Merge queue</b>
                <span className="cx-s">Re-checks</span>
                <span className="cx-s">combined</span>
                <span className="cx-st"><i className="cx-sd" />Waiting</span>
              </div>
              <div className="cx-fln cx-cpend" style={{ left: "904px", top: "168px", height: "80px" }}><b>Main</b><span className="cx-s">Watched</span><span className="cx-st"><i className="cx-sd" />Waiting</span></div>
              <div className="cx-fln cx-cpend" style={{ left: "1017px", top: "168px", height: "80px" }}>
                <b>You</b>
                <span className="cx-s">Review</span>
                <span className="cx-s">after merge</span>
                <span className="cx-st"><i className="cx-sd" />Waiting</span>
              </div>
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
                <span className="cx-played" style={{ width: "76%" }} />
                <i className="cx-g" style={{ left: "5%" }} />
                <i className="cx-g" style={{ left: "14%" }} />
                <i className="cx-g" style={{ left: "22%" }} />
                <i className="cx-g" style={{ left: "34%" }} />
                <i className="cx-g" style={{ left: "44%" }} />
                <i className="cx-g" style={{ left: "56%" }} />
                <i className="cx-b" style={{ left: "66%" }} />
                <i className="cx-b" style={{ left: "72%" }} />
                <span className="cx-ph2" style={{ left: "76%" }} />
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
