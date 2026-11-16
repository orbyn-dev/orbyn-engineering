import { Frame } from "../shared/Frame";
import { Seg } from "../shared/parts";

/** Crew Designs, frame `flow-empty-crews`: Flow: no crews running. */
export function FlowEmptyCrews() {
  return (
    <Frame height={700}>
      <div className="cx-cwpg">
            <div className="cx-ph">
              <h3>Crew<span className="cx-cwpv">Preview</span></h3>
            </div>
            <div className="cx-cwtabs"><Seg kind="segx" items={["Flow", "Review", "Map", "Board", "Lanes", "Inbox", "Report"]} on={0} /></div>
            <div style={{ padding: "16px 20px", color: "var(--color-muted)" }}>
              <p style={{ margin: "0", color: "var(--color-text)", fontSize: "14px", fontWeight: 500 }}>No crews running</p>
            </div>
      </div>
    </Frame>
  );
}
