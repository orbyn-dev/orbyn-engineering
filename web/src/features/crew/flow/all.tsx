import { Frame } from "../shared/Frame";
import { PageHead, Seg } from "../shared/parts";
import { Ic } from "./icons";

/** Crew Designs, frame `flow-all`: Flow: the whole crew. */
export function FlowAll() {
  return (
    <Frame width={936} height={700}>
      <div className="cx-cwpg">
            <PageHead sub="Login redirect crew · Lead: Fix login redirect on Claude Code · 8 of 31 tasks" acts />
            <div className="cx-cwtabs">
              <Seg kind="segx" items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} on={0} />
              <span className="cx-end">Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m</span>
            </div>
            <div className="cx-cwtool">
              <span className="cx-srch"><Ic n="search" sm />Find a task</span>
              <span className="cx-fil cx-on">All 31</span>
              <span className="cx-fil"><Ic n="alert" sm />Needs you 2</span>
              <span className="cx-fil">Reviewing 5</span>
              <span className="cx-fil">Failed 2</span>
              <span className="cx-fil">Passed 5</span>
              <span className="cx-fil"><Ic n="down" sm />Engine: any</span>
              <span className="cx-end" style={{ marginLeft: "auto" }}>8 of 31 shown</span>
            </div>
            <div className="cx-fta">
              <div className="cx-r cx-h" style={{ gridTemplateColumns: "minmax(0,230px) 58px 58px 58px 58px 58px 58px 58px 58px 58px 58px 74px" }}>
                <span>Task</span>
                <span style={{ textAlign: "center", whiteSpace: "nowrap" }}>Brief</span>
                <span style={{ textAlign: "center", whiteSpace: "nowrap" }}>Route</span>
                <span style={{ textAlign: "center", whiteSpace: "nowrap" }}>Build</span>
                <span style={{ textAlign: "center", whiteSpace: "nowrap" }}>Checks</span>
                <span style={{ textAlign: "center", whiteSpace: "nowrap" }}>Code</span>
                <span style={{ textAlign: "center", whiteSpace: "nowrap" }}>Security</span>
                <span style={{ textAlign: "center", whiteSpace: "nowrap" }}>Finding</span>
                <span style={{ textAlign: "center", whiteSpace: "nowrap" }}>Merge</span>
                <span style={{ textAlign: "center", whiteSpace: "nowrap" }}>Main</span>
                <span style={{ textAlign: "center", whiteSpace: "nowrap" }}>You</span>
                <span style={{ textAlign: "center" }}>Round</span>
              </div>
              <div className="cx-r cx-sel" style={{ gridTemplateColumns: "minmax(0,230px) 58px 58px 58px 58px 58px 58px 58px 58px 58px 58px 74px" }}>
                <div><b>Use next in the auth redirect</b><small>implement · Claude Code · owner Lead</small></div>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm />3/3</span>
                <span className="cx-fc cx-run"><i className="cx-spin" aria-label="Working" />{" ×3"}</span>
                <span className="cx-fc cx-run"><i className="cx-spin" aria-label="Working" />{" ×1"}</span>
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-run" style={{ minWidth: "64px" }}>Round 1</span>
              </div>
              <div className="cx-r" style={{ gridTemplateColumns: "minmax(0,230px) 58px 58px 58px 58px 58px 58px 58px 58px 58px 58px 74px" }}>
                <div><b>Add tests for the redirect</b><small>test · Codex</small></div>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-run"><i className="cx-spin" aria-label="Working" />2/3</span>
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-run" style={{ minWidth: "64px" }}>Round 1</span>
              </div>
              <div className="cx-r" style={{ gridTemplateColumns: "minmax(0,230px) 58px 58px 58px 58px 58px 58px 58px 58px 58px 58px 74px" }}>
                <div><b>Review the diff for open redirects</b><small>review · OpenCode</small></div>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm />2/2</span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm />{" ×2"}</span>
                <span className="cx-fc cx-need"><Ic n="alert" sm />{" ×1"}</span>
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-pend" />
                <span className="cx-fc" style={{ minWidth: "64px" }}>Round 1</span>
              </div>
              <div className="cx-r" style={{ gridTemplateColumns: "minmax(0,230px) 58px 58px 58px 58px 58px 58px 58px 58px 58px 58px 74px" }}>
                <div><b>Move session helpers to auth/session.ts</b><small>implement · Claude Code</small></div>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm />4/4</span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm />{" ×2"}</span>
                <span className="cx-fc cx-skip">–</span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-rev"><Ic n="eye" sm /></span>
                <span className="cx-fc" style={{ minWidth: "64px" }}>Round 1</span>
              </div>
              <div className="cx-r" style={{ gridTemplateColumns: "minmax(0,230px) 58px 58px 58px 58px 58px 58px 58px 58px 58px 58px 74px" }}>
                <div><b>Rename the legacy returnTo helper</b><small>implement · Claude Code</small></div>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-fail"><Ic n="alert" sm />2/3</span>
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-fail" style={{ minWidth: "64px" }}>Round 3</span>
              </div>
              <div className="cx-r" style={{ gridTemplateColumns: "minmax(0,230px) 58px 58px 58px 58px 58px 58px 58px 58px 58px 58px 74px" }}>
                <div><b>Update the login docs</b><small>docs · Codex</small></div>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm />2/2</span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm />{" ×2"}</span>
                <span className="cx-fc cx-skip">–</span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-rev"><Ic n="eye" sm /></span>
                <span className="cx-fc" style={{ minWidth: "64px" }}>Round 1</span>
              </div>
              <div className="cx-r" style={{ gridTemplateColumns: "minmax(0,230px) 58px 58px 58px 58px 58px 58px 58px 58px 58px 58px 74px" }}>
                <div><b>Add rate limit to the login route</b><small>implement · Codex</small></div>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm />5/5</span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm />{" ×3"}</span>
                <span className="cx-fc cx-fail"><Ic n="alert" sm />{" ×2"}</span>
                <span className="cx-fc cx-fail"><Ic n="alert" sm /></span>
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-fail" style={{ minWidth: "64px" }}>Round 2</span>
              </div>
              <div className="cx-r" style={{ gridTemplateColumns: "minmax(0,230px) 58px 58px 58px 58px 58px 58px 58px 58px 58px 58px 74px" }}>
                <div><b>Move the CHANGELOG entry</b><small>docs · Claude Code</small></div>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-ok"><Ic n="ok" sm /></span>
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-pend" />
                <span className="cx-fc cx-pend" />
                <span className="cx-fc" style={{ minWidth: "64px" }}>Queued</span>
              </div>
            </div>
            <div className="cx-cwcap" style={{ padding: "8px 28px" }}>+23 more. Scroll inside the list; a row that needs you rises to the top.</div>
            <div className="cx-flk" style={{ marginTop: "auto" }}>
              <span>Cell:</span>
              <span className="cx-fc cx-ok" style={{ minWidth: "34px" }}><Ic n="ok" sm /></span>
              <span className="cx-fc cx-run" style={{ minWidth: "34px" }}><i className="cx-spin" aria-label="Working" /></span>
              <span className="cx-fc cx-need" style={{ minWidth: "34px" }}><Ic n="alert" sm /></span>
              <span className="cx-fc cx-fail" style={{ minWidth: "34px" }}><Ic n="alert" sm /></span>
              <span className="cx-fc cx-pend" style={{ minWidth: "34px" }} />
              <span className="cx-fc cx-skip" style={{ minWidth: "34px" }}>–</span>
              <span className="cx-fc cx-rev" style={{ minWidth: "34px" }}><Ic n="eye" sm /></span>
              <span>done · running · needs you · failed · not reached · skipped · waiting for your review after the merge</span>
              <span style={{ marginLeft: "auto" }}>×N: reviewer lanes that ran</span>
            </div>
      </div>
    </Frame>
  );
}
