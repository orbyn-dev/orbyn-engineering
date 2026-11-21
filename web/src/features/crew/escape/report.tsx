import { Planned } from "../capacity/Planned";
import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** The mark's Planned fields, in the words their pills give. */
const WHY = {
  model: "the merge does not record a model or a lane yet, so a row names the engine and the area only",
  field: "a mark keeps one reason of at most 200 characters, not this field",
  review: "the beta has no Review tab; this view is drawn here and is reached another way",
} as const;

/**
 * Crew Designs v79, frame `escape-report`: the Review item's "Mark as an escape" dialog. The mark and its note are built
 * (a Person route), but the beta has no Review queue to open the dialog from, and the how-found and fix fields are
 * Planned, so every merge, engine, area and date here is the design's sample. Nothing on this screen sends anything.
 */
export function EscapeReport() {
  return (
    <Frame height={1010}>
      <div className="cx-cwpg">
        <PageHead sub="Login redirect crew · Lead: Fix login redirect on claude · 3 workers" acts />
        <div className="cx-cwtabs">
          <span className="cx-segx">
            {["Approvals", "Trust", "Background", "Inbox", "Flow", "Board", "Map", "Lanes"].map((label) => (
              <span key={label}>{label}</span>
            ))}
            <span className="cx-on">
              Review
              <Planned why={WHY.review} style={{ height: "16px", verticalAlign: "middle" }} />
            </span>
          </span>
          <span className="cx-end">claude 412k of 1M tokens · codex 62% of its weekly limit · 2h 14m</span>
        </div>
        <div className="cx-cwtool">
          <Seg kind="cwseg" on={0} items={["Review queue · 7", "Merge queue · 3", "Digest", "Trust", "Dashboard", "Background"]} />
          <span style={{ marginLeft: "auto" }}>Main is healthy · watched since 09:00</span>
        </div>
        <div className="cx-cwtool" style={{ borderTop: "0" }}>
          <span className="cx-fil cx-on">To review 7</span>
          <span className="cx-fil">High 1</span>
          <span className="cx-fil">Overdue 0</span>
          <span className="cx-fil">Reverted 1</span>
          <span className="cx-fil"><Ic n="down" sm />Engine: any</span>
          <span style={{ marginLeft: "auto" }}>Ranked by risk, then by the time you have left</span>
        </div>
        <div style={{ padding: "4px 20px 12px", display: "flex", flexDirection: "column", gap: "8px", overflow: "hidden", minHeight: "0" }}>
          <div className="cx-apq">
            <div className="cx-rt"><span className="cx-aprk cx-rmd">Medium</span></div>
            <div className="cx-tt">
              <b>Restore the rate limit on the legacy login route</b>
              <div className="cx-m">
                {"Merged by Crew at 16:10 · codex · GPT-5.5 · crew/login-limit · web/auth/login.ts "}
                <span className="cx-t13mk"><Ic n="restore" sm />web/auth is on On: 2 reverts in 14 days</span>
              </div>
              <div className="cx-pf">
                <span><Ic n="ok" sm />Checks 4 of 4</span>
                <span><Ic n="eye" sm />Reviewed by 2 lanes · 1 vendor</span>
                <span><Ic n="bug" sm />0 findings</span>
                <span><Ic n="info" sm />Not verified: 1</span>
                <span><Ic n="clock" sm />32k tokens · 14 min</span>
              </div>
            </div>
            <div className="cx-ac"><Btn className="cx-btn">Looks good</Btn><Btn className="cx-ghost">Comment</Btn><Btn className="cx-ghost">Revert</Btn></div>
            <div className="cx-rb">Review by Sun 8 Nov, 16:10 · 23 h left</div>
          </div>
          <div className="cx-apq">
            <div className="cx-rt"><span className="cx-aprk cx-rmd">Medium</span></div>
            <div className="cx-tt">
              <b>Use next in the auth redirect</b>
              <div className="cx-m">Merged by Crew at 14:32 · claude · Sonnet 5.5 · crew/redirect-impl · web/auth/redirect.ts</div>
              <div className="cx-pf">
                <span><Ic n="ok" sm />Checks 3 of 3</span>
                <span><Ic n="eye" sm />Reviewed by 2 lanes · 2 vendors</span>
                <span><Ic n="bug" sm />0 findings</span>
                <span><Ic n="info" sm />Not verified: 4</span>
                <span><Ic n="clock" sm />52k tokens · 22 min</span>
              </div>
            </div>
            <div className="cx-ac"><Btn className="cx-btn">Looks good</Btn><Btn className="cx-ghost">Comment</Btn><Btn className="cx-ghost">Revert</Btn></div>
            <div className="cx-rb">Review by Sun 8 Nov, 14:32 · 22 h left</div>
          </div>
          <div className="cx-apq">
            <div className="cx-rt"><span className="cx-aprk cx-rlo">Low</span></div>
            <div className="cx-tt">
              <b>Update the login docs</b>
              <div className="cx-m">Merged by Crew at 14:12 · claude · Sonnet 5.5 · docs/guides/login.md</div>
              <div className="cx-pf">
                <span><Ic n="ok" sm />Checks 4 of 4</span>
                <span><Ic n="eye" sm />Reviewed by 2 lanes · 1 vendor</span>
                <span><Ic n="bug" sm />0 findings</span>
                <span><Ic n="info" sm />Not verified: 1</span>
                <span><Ic n="clock" sm />32k tokens · 14 min</span>
              </div>
            </div>
            <div className="cx-ac"><Btn className="cx-btn">Looks good</Btn><Btn className="cx-ghost">Comment</Btn><Btn className="cx-ghost">Revert</Btn></div>
            <div className="cx-rb">Batch review · no deadline</div>
          </div>
        </div>
        <div className="cx-cwov">
          <div>
            <div className="cx-cwdlg" style={{ width: "600px" }}>
              <h4>Mark this merge as an escape</h4>
              <p>
                Use this when a merge reached main and turned out to have a bug, and you are not reverting it. Your mark is
                the confirm: the escape is confirmed at once and counts against the engine and the area. A revert is
                already in the log as a possible escape.
              </p>
              <div className="cx-fbx">
                <div className="cx-fkv">
                  <span>Merge</span>
                  <b>Use next in the auth redirect · c7f20b9 · Sat 7 Nov, 14:32</b>
                  <span>Counts against</span>
                  <b>
                    claude · web/auth
                    <span
                      className="cx-cwpv"
                      style={{ height: "16px", marginLeft: "6px", whiteSpace: "nowrap" }}
                      title={`Planned: ${WHY.model}`}
                    >
                      Model: Planned
                    </span>
                  </b>
                </div>
              </div>
              <div className="cx-fld2">
                <span>What went wrong <span className="cx-rtag cx-lo">Optional</span></span>
                <span className="cx-inp" style={{ fontFamily: "var(--font-sans)" }}>The legacy login route lost its rate limit.</span>
              </div>
              <div className="cx-fld2">
                <span>
                  How it was found
                  <Planned why={WHY.field} style={{ height: "16px", whiteSpace: "nowrap" }} />
                </span>
                <div className="cx-t14opt cx-on"><Ic n="ok" sm /><div><b>Found by me</b><small>You saw it in a review or on main</small></div></div>
                <div className="cx-t14opt">
                  <span style={{ width: "14px", flex: "none" }} />
                  <div><b>Found by a later check or a scan</b><small>A check, a watcher or a reviewer lane saw it after the merge</small></div>
                </div>
                <div className="cx-t14opt">
                  <span style={{ width: "14px", flex: "none" }} />
                  <div><b>Reported by someone else</b><small>A user, a teammate or an incident</small></div>
                </div>
              </div>
              <div className="cx-fld2">
                <span>
                  The fix <span className="cx-rtag cx-lo">Optional</span>
                  <Planned why={WHY.field} style={{ height: "16px", whiteSpace: "nowrap" }} />
                </span>
                <span className="cx-inp" style={{ fontFamily: "var(--font-sans)" }}>Restore the rate limit on the legacy login route · merged 16:10</span>
              </div>
              <p>
                The note is the escape’s reason: at most 200 characters, with secrets removed. If this merge already has a
                possible escape, your mark confirms it and its source stays. If it is already confirmed nothing changes. If
                you dismissed it before, the mark is refused. There is no undo in v1.
              </p>
              <div className="cx-ra"><Btn className="cx-ghost">Cancel</Btn><Btn className="cx-btn">Mark as an escape</Btn></div>
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
