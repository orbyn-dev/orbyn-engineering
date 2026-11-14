import { Frame } from "../shared/Frame";
import { Btn } from "../shared/parts";
import { Ic } from "./icons";

/** Crew Designs, frame `flow-narrow-panel`: Flow in a 320 px panel. */
export function FlowNarrowPanel() {
  return (
    <Frame panel height={700}>
        <div className="cx-ptabs cx-btabs cx-ico" aria-label="Panel tabs">
          <span className="cx-ptab" aria-label="Changes" title="Changes"><Ic n="diff" sm /></span>
          <span className="cx-ptab" aria-label="Activity" title="Activity"><Ic n="activity" sm /></span>
          <span className="cx-ptab" aria-label="Side chat" title="Side chat"><Ic n="side" sm /></span>
          <span className="cx-ptab cx-on" aria-current="true" aria-label="Crew" title="Crew"><Ic n="flow" sm /></span>
          <span className="cx-padd" aria-label="Open a tab"><Ic n="plus" sm /></span>
          <span className="cx-end" style={{ marginLeft: "auto", display: "flex" }}><Btn className="cx-icb" aria-label="Maximise panel"><Ic n="max" sm /></Btn><Btn className="cx-icb" aria-label="Close side panel"><Ic n="x" sm /></Btn></span>
        </div>
        <div className="cx-phd2"><b>Crew</b><span className="cx-cwpv">Preview</span><span className="cx-pha"><Btn className="cx-ghost" style={{ height: "24px", fontSize: "12px", padding: "0 9px" }}>Open in Crew</Btn><Btn className="cx-icb" aria-label="Tab menu"><Ic n="more" sm /></Btn></span></div>
        <div className="cx-cwpseg" style={{ padding: "8px 10px", flex: "none" }}>
          <span className="cx-segx" aria-label="Crew views">
            <span aria-current="true" className="cx-on">Flow</span>
            <span>Review</span>
            <span>Map</span>
            <span>Board</span>
            <span>Lanes</span>
            <Btn className="cx-ovf" aria-label="More Crew views, 1 with a count"><Ic n="more" sm /><i className="cx-ovc" aria-hidden="true">4</i></Btn>
          </span>
        </div>
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
            <span className="cx-n"><span className="cx-st cx-info"><i className="cx-spin" role="img" aria-label="Working" />2 of 3</span></span>
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
            <span className="cx-n"><span className="cx-st cx-info"><i className="cx-spin" role="img" aria-label="Working" />Reviewing</span></span>
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
    </Frame>
  );
}
