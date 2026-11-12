import { Frame } from "../shared/Frame";
import { Btn } from "../shared/parts";

/** Crew Designs v23, frame `settings-crew`. */
export function SettingsCrew() {
  return (
    <Frame sheet>
      <div className="cx-cwsheet" style={{ maxWidth: "760px" }}>
        <h3 style={{ margin: "0", fontSize: "16px", fontWeight: 500, color: "var(--color-text)" }}>Crew<span className="cx-cwpv">Preview</span></h3>
        <div>
          <div className="cx-setrow">
            <div>
              <b>Default budget for a new crew</b>
              <small>Used when you start a crew without setting one. Each engine counts in its own unit, so there is no single percentage.</small>
            </div>
            <span className="cx-sv">Claude Code 1M tokens · Codex 800k tokens</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>Questions that can wait</b>
              <small>
                Low-risk questions are batched and wait up to this long before they interrupt you. Irreversible, security, money and data-loss questions interrupt at once, whatever this says.
              </small>
            </div>
            <span className="cx-sv">10 minutes</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>Review on the same engine</b>
              <small>
                When no engine but the author’s is signed in, Crew asks you first. This lets it go ahead without asking; the review is always labelled “Same engine as author”.
              </small>
            </div>
            <span className="cx-sv">Ask me before it runs</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>Review windows after a merge</b>
              <small>
                How long you have to review what Crew merged under Autopilot. A High change sits at the top of the Review queue and raises an overdue alert when its window passes. Low changes are batched with no deadline and listed in the digest.
              </small>
            </div>
            <span className="cx-sv">High 4 hours · Medium 24 hours · Low no deadline</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>Review capacity</b>
              <small>
                How many High and Medium changes you can have waiting before Crew pauses new Low-priority starts, and whether low-risk batches are sampled. Open Review capacity.
              </small>
            </div>
            <span className="cx-sv">Not set</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>Independent oracle</b>
              <small>
                Who writes the answer key (an engine other than the builder) and whether a mutation gate checks that the key can catch a break. Open Independent oracle.
              </small>
            </div>
            <span className="cx-sv">Another engine · gate on</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>Reviewer backtest</b>
              <small>
                Replay your review lanes and checks on merged pull requests whose outcome is known, to see what each would have caught. Needs a run budget for each engine. Open Reviewer backtest.
              </small>
            </div>
            <span className="cx-sv">Budget not set</span>
          </div>
          <div className="cx-setrow">
            <div><b>Daily digest</b><small>What merged, what is waiting for your review and what was reverted, sent once a day in your local time.</small></div>
            <span className="cx-sv">17:00</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>Digest notification</b>
              <small>A notification when the digest is ready. There is none on a day when nothing happened; the digest is still there to open.</small>
            </div>
            <span className="cx-sv">On</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>Default budget for background crews</b>
              <small>
                Set when a background crew is created, in each engine’s own unit. A run that reaches it stops with the reason “budget” and lands in the Review queue with what it finished.
              </small>
            </div>
            <span className="cx-sv">Claude Code 300k tokens · Codex 250k tokens</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>Exchanges before the lead is told</b>
              <small>How many messages two members may trade before the lead steps in. A repo rule can lower it.</small>
            </div>
            <span className="cx-sv cx-ro">3 · Default</span>
          </div>
          <div className="cx-setrow">
            <div><b>Posts per member in one run</b><small>A limit that stops a runaway conversation. A repo rule can lower it, never raise it.</small></div>
            <span className="cx-sv cx-ro">30 · Default</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>Engine tiers</b>
              <small>
                The tier each engine fills when Crew routes work: Lead, Builder or Scout. Claude Code and Codex fill the tiers their own models give. For an engine that connects over ACP, you map the tier here. Crew never guesses a tier from a model name; an engine you haven’t mapped shows “Builder · not mapped” until you do.
              </small>
            </div>
            <span className="cx-sv">3 engines</span>
          </div>
          <div className="cx-setrow cx-sub">
            <div><b>OpenCode</b><small>Connects over ACP. Mapped by you.</small></div>
            <span className="cx-sv">Builder</span>
          </div>
          <div className="cx-setrow cx-sub">
            <div><b>Cursor</b><small>Connects over ACP. Not mapped yet, so it is treated as Builder and marked “not mapped” wherever its tier shows.</small></div>
            <span className="cx-sv">{"Builder · not mapped "}<u>Map it</u></span>
          </div>
          <div className="cx-setrow cx-sub">
            <div><b>Kimi CLI</b><small>Not installed.</small></div>
            <span className="cx-sv" style={{ color: "var(--color-muted)" }}>Not installed</span>
          </div>
          <div className="cx-setrow">
            <div>
              <b>Repo rules</b>
              <small>Rules live in .orbyn/crew.json in each repo and can add stages or tighten these limits. Personal defaults are on this page.</small>
            </div>
            <Btn className="cx-ghost" style={{ height: "26px", fontSize: "12px" }}>Open in orbyn-engineering</Btn>
          </div>
        </div>
      </div>
    </Frame>
  );
}
