import { Frame } from "../shared/Frame";
import { Btn, PageHead, Seg } from "../shared/parts";
import { Ic } from "./icons";

/** Crew Designs, frame `flow-narrow-window`: Flow in a narrow window (640 px). */
export function FlowNarrowWindow() {
  return (
    <Frame width={640} height={700}>
      <div className="cx-cwpg">
            <PageHead sub="Login redirect crew · 5 runs" />
            <div className="cx-cwtabs" style={{ padding: "8px 14px", overflow: "hidden" }}><Seg kind="segx" items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} on={0} /></div>
            <div style={{ padding: "8px 14px", display: "flex", gap: "8px", alignItems: "center", flex: "none" }}><b style={{ fontWeight: 500, color: "var(--color-text)" }}>Use next in the auth redirect</b><span className="cx-fbadge">Round 1</span><span className="cx-end" style={{ marginLeft: "auto" }}><Btn className="cx-ghost" style={{ height: "24px", padding: "0 9px" }}>Replay</Btn></span></div>
            <div className="cx-fsm cx-c">
              <div className="cx-row">
                <Ic n="right" sm />
                <Ic n="brief" sm />
                <b>Brief</b>
                <span className="cx-n"><span className="cx-st cx-ok"><i className="cx-sd" />Written</span></span>
              </div>
              <div className="cx-row">
                <Ic n="right" sm />
                <Ic n="gate" sm />
                <b>Route</b>
                <span className="cx-n"><span className="cx-st cx-ok"><i className="cx-sd" />Routed</span></span>
              </div>
              <div className="cx-row">
                <Ic n="right" sm />
                <Ic n="wrench" sm />
                <b>Build</b>
                <span className="cx-n"><span className="cx-st cx-ok"><i className="cx-sd" />Done</span></span>
              </div>
              <div className="cx-row">
                <Ic n="right" sm />
                <Ic n="check2" sm />
                <b>Checks</b>
                <span className="cx-n"><span className="cx-st cx-ok"><i className="cx-sd" />3 of 3</span></span>
              </div>
              <div className="cx-row">
                <Ic n="down" sm />
                <Ic n="eye" sm />
                <b>Code review</b>
                <span className="cx-n"><span className="cx-st cx-info"><i className="cx-spin" aria-label="Working" />2 of 3</span></span>
              </div>
              <div className="cx-sub">
                <div className="cx-it"><span style={{ minWidth: "0", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>Correctness · Codex</span><span className="cx-n">No findings</span></div>
                <div className="cx-it"><span style={{ minWidth: "0", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>Performance · OpenCode</span><span className="cx-n">Reviewing</span></div>
                <div className="cx-it"><span style={{ minWidth: "0", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>Docs · OpenCode</span><span className="cx-n">1 unconfirmed</span></div>
              </div>
              <div className="cx-row">
                <Ic n="right" sm />
                <Ic n="shield" sm />
                <b>Security review</b>
                <span className="cx-n"><span className="cx-st cx-info"><i className="cx-spin" aria-label="Working" />Reviewing</span></span>
              </div>
              <div className="cx-row">
                <Ic n="right" sm />
                <Ic n="scan" sm />
                <b>Finding check</b>
                <span className="cx-n"><span className="cx-st"><i className="cx-sd" />Waiting</span></span>
              </div>
              <div className="cx-row">
                <Ic n="right" sm />
                <Ic n="branch" sm />
                <b>Merge queue</b>
                <span className="cx-n"><span className="cx-st"><i className="cx-sd" />Waiting</span></span>
              </div>
              <div className="cx-row">
                <Ic n="right" sm />
                <Ic n="git" sm />
                <b>Main</b>
                <span className="cx-n"><span className="cx-st"><i className="cx-sd" />Watched</span></span>
              </div>
              <div className="cx-row">
                <Ic n="right" sm />
                <Ic n="user" sm />
                <b>You</b>
                <span className="cx-n"><span className="cx-st"><i className="cx-sd" />After the merge</span></span>
              </div>
            </div>
      </div>
    </Frame>
  );
}
