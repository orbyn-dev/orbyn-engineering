import { Frame } from "../shared/Frame";
import { Btn, Ic } from "../shared/parts";

/** Crew Designs v23, frame `backtest-settings`. */
export function BacktestSettings() {
  return (
    <Frame sheet>
      <div className="cx-cwsheet" style={{ maxWidth: "760px" }}>
        <h3 style={{ margin: "0", fontSize: "16px", fontWeight: 500, color: "var(--color-text)" }}>
          Crew › Reviewer backtest
          <span className="cx-cwpv">Preview</span>
        </h3>
        <p style={{ margin: "0", color: "var(--color-muted)", fontSize: "12.5px", lineHeight: 1.5 }}>
          Which of your review lanes, and which engines, would have caught the changes that later turned out wrong in this repo? A backtest replays them on your own history and shows the counts.
        </p>
        <div className="cx-cwbn" style={{ margin: "0" }}>
          <Ic n="info" sm />
          <span>Last backtest: Sat 7 Nov, 64 PRs, 9 escapes. It used 1.28M tokens of Claude Code, 896k tokens of Codex and 396k tokens of OpenCode.</span>
          <span className="cx-acts"><Btn className="cx-ghost">Open the report</Btn><Btn className="cx-btn">Run a backtest</Btn></span>
        </div>
        <div>
          <div className="cx-setrow">
            <div>
              <b>What it replays</b>
              <small>
                The review lanes and the repo’s checks, on merged pull requests whose outcome is known. Never the builder: building isn’t repeatable, so a replay of it would measure nothing.
              </small>
            </div>
            <span className="cx-sv cx-ro">Reviews and checks</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>Which pull requests</b>
              <small>
                Merged PRs from the last 180 days that are 30 days old or more, so a revert or a fix has had time to happen. A PR escaped if it was reverted, or you marked its merge as an escape in the escape log. A merge you marked Not an escape counts as no escape.
              </small>
            </div>
            <span className="cx-sv">Last 180 days · Proposed</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>Which lanes</b>
              <small>
                Every review lane in the repo’s lane rules, and the checks. A lane replays on the PRs it would have run on: the security lane on changes under auth/ or keys/.
              </small>
            </div>
            <span className="cx-sv cx-ro">All lanes and the checks</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>Run budget</b>
              <small>
                A backtest uses engines the way a real run does, so each engine has a budget in its own unit, never added up: Claude Code 1.5M tokens, Codex 1M tokens and OpenCode 500k tokens. Crew never goes past a budget.
              </small>
            </div>
            <span className="cx-sv">3 budgets · Proposed</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>When it runs</b>
              <small>
                Crew starts it when you choose Run a backtest. It shows an estimate for each engine before it runs, and the run stops by itself if it would pass a budget.
              </small>
            </div>
            <span className="cx-sv cx-ro">When you start it</span>
          </div>
        </div>
        <div className="cx-fbx">
          <h6>What this tells you, and what it doesn’t</h6>
          <p>
            It replays the review lanes and the checks on pull requests that already merged, as they were at merge time, and compares them with what happened next. It never replays the builder: building isn’t repeatable, so a replay of it would measure nothing. A model’s review can differ from one run to the next, so each replay is one sample, not a promise. Crew makes no claim that one engine, or one vendor, reviews better. This is what happened on this repo, with the sample under every figure.
          </p>
        </div>
        <div className="cx-t13row">
          <span className="cx-t13mk">Sample data</span>
          <span className="cx-t13mk">Proposed: Engineering sets the real values</span>
        </div>
        <p style={{ margin: "2px 0 0", display: "flex", gap: "6px", alignItems: "flex-start", color: "var(--color-muted)", fontSize: "12px", lineHeight: 1.45 }}>
          <Ic n="info" sm />
          <span>Workers run as you, with your machine’s permissions.</span>
        </p>
      </div>
    </Frame>
  );
}
