import { Frame } from "../shared/Frame";
import { Btn, Seg } from "../shared/parts";
import { Ic } from "./icons";

/** Crew Designs, frame `flow-source-error`: Flow: can’t read Flow. */
export function FlowSourceError() {
  return (
    <Frame height={700}>
      <div className="cx-cwpg">
            <div className="cx-ph">
              <h3>Crew<span className="cx-cwpv">Preview</span></h3>
            </div>
            <div className="cx-cwtabs"><Seg kind="segx" items={["Flow", "Review", "Map", "Board", "Lanes", "Inbox", "Report"]} on={0} /></div>
            <div style={{ padding: "16px 20px" }}>
              <div className="cx-cwbn cx-red" style={{ margin: "0" }}><span style={{ color: "var(--color-danger)", flex: "none", display: "inline-flex" }}><Ic n="alert" sm /></span><span style={{ display: "flex", flexDirection: "column", minWidth: "0", flex: 1 }}><b style={{ fontWeight: 600 }}>Can’t read Flow</b><span style={{ color: "var(--color-textSoft)" }}>Check that the Orbyn service is running.</span></span><span className="cx-acts"><Btn className="cx-btn" style={{ height: "26px" }}>Retry</Btn></span></div>
            </div>
      </div>
    </Frame>
  );
}
