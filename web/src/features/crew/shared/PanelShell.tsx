import type { ReactNode } from "react";
import { Btn, Ic } from "./parts";

const TABS = [
  ["Changes", "diff"],
  ["Activity", "activity"],
  ["Side chat", "side"],
  ["Crew", "flow"],
] as const;
const VIEWS = ["Flow", "Review", "Map", "Board", "Lanes"] as const;

/** The side panel at 320 px as drawn: its tabs with Crew selected, the Crew header and the Crew views. `on` is the view drawn as chosen; the last view is Inbox 4, or Report when that is the chosen one. */
export function PanelShell({ on, children }: { on: "Review" | "Report"; children: ReactNode }) {
  return (
    <>
      <div className="cx-ptabs cx-btabs cx-ico">
        {TABS.map(([label, icon]) => (
          <span key={label} className={label === "Crew" ? "cx-ptab cx-on" : "cx-ptab"} aria-current={label === "Crew" ? "true" : undefined} aria-label={label} title={label}>
            <Ic n={icon} sm />
          </span>
        ))}
        <span className="cx-padd" aria-label="Open a tab">
          <Ic n="plus" sm />
        </span>
        <span className="cx-end" style={{ marginLeft: "auto", display: "flex" }}>
          <Btn className="cx-icb" aria-label="Maximise panel">
            <Ic n="max" sm />
          </Btn>
          <Btn className="cx-icb" aria-label="Close side panel">
            <Ic n="x" sm />
          </Btn>
        </span>
      </div>
      <div className="cx-phd2">
        <b>Crew</b>
        <span className="cx-cwpv">Preview</span>
        <span className="cx-pha">
          <Btn className="cx-ghost" style={{ height: "24px", fontSize: "12px", padding: "0 9px" }}>
            Open in Crew
          </Btn>
          <Btn className="cx-icb" aria-label="Tab menu">
            <Ic n="more" sm />
          </Btn>
        </span>
      </div>
      <div className="cx-cwpseg" style={{ padding: "8px 10px", flex: "none" }}>
        <span className="cx-segx" aria-label="Crew views">
          {[...VIEWS, on === "Report" ? "Report" : "Inbox 4"].map((name) => (
            <span key={name} className={name === on ? "cx-on" : undefined} aria-current={name === on ? "true" : undefined}>
              {name}
            </span>
          ))}
          <Btn className="cx-ovf" aria-label="More Crew views" title="More Crew views">
            <Ic n="more" sm />
          </Btn>
        </span>
      </div>
      {children}
    </>
  );
}
