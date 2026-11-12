import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v23, frame `backtest-lane-picker`. */
export function BacktestLanePicker() {
  return (
    <Frame height={760}>
      <div className="cx-cwpg">
        <PageHead sub="Login redirect crew · Lead: Fix login redirect on Claude Code · 3 workers" acts />
        <div className="cx-cwtabs">
          <Seg kind="segx" on={4} items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} />
          <span className="cx-end">Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m</span>
        </div>
        <div style={{ padding: "10px 20px", display: "flex", flexDirection: "column", gap: "8px", overflow: "hidden", minHeight: "0" }}>
          <div className="cx-t14card">
            <b>Round 1</b>
            <small>Correctness · Codex · Added by the brief</small>
            <small>Security · Codex · lead tier · Done</small>
          </div>
        </div>
        <div className="cx-cwov">
          <div>
            <div className="cx-cwdlg" style={{ width: "620px" }}>
              <h4>Add a reviewer: choose an engine</h4>
              <p>Each engine shows what the last backtest found on this repo, with its sample. It is information: it doesn’t change which engine Auto picks.</p>
              <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
                <div className="cx-t14opt cx-on">
                  <Ic n="check2" sm />
                  <span><b>Auto · a different engine from the author</b><small>Picks the engine by the route rules, and says why on the route card</small></span>
                </div>
                <div className="cx-t14opt">
                  <span style={{ width: "14px", flex: "none" }} />
                  <span>
                    <b>Claude Code · Sonnet 5.5</b>
                    <small>Backtest, Sat 7 Nov: caught 6 of 9 past escapes · 6 of 21 findings matched · 64 pull requests</small>
                  </span>
                </div>
                <div className="cx-t14opt">
                  <span style={{ width: "14px", flex: "none" }} />
                  <span><b>Codex · GPT-5.5</b><small>Backtest, Sat 7 Nov: caught 5 of 9 past escapes · 5 of 17 findings matched · 64 pull requests</small></span>
                </div>
                <div className="cx-t14opt">
                  <span style={{ width: "14px", flex: "none" }} />
                  <span><b>OpenCode · DeepSeek V4</b><small>Backtest, Sat 7 Nov: security lane · caught 3 of 4 past escapes · 22 pull requests</small></span>
                </div>
                <div className="cx-t14opt">
                  <span style={{ width: "14px", flex: "none" }} />
                  <span><b>Cursor · Auto</b><small>Not measured: it wasn’t in the last backtest</small></span>
                </div>
              </div>
              <p style={{ fontSize: "12px" }}>
                Counts, not a ranking: with 9 escapes, one escape more or less changes the order. Crew makes no claim that one engine reviews better.
              </p>
              <div className="cx-ra"><Btn className="cx-ghost">Cancel</Btn><Btn className="cx-btn">Add reviewer</Btn></div>
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
