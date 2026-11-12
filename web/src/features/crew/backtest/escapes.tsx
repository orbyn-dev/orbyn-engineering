import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v23, frame `backtest-escapes`. */
export function BacktestEscapes() {
  return (
    <Frame>
      <div className="cx-cwpg">
        <PageHead sub="Login redirect crew · Lead: Fix login redirect on Claude Code · 3 workers" acts />
        <div className="cx-cwtabs">
          <Seg kind="segx" on={1} items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} />
          <span className="cx-end">Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m</span>
        </div>
        <div className="cx-cwtool">
          <Seg kind="cwseg" on={4} items={["Review queue · 7", "Merge queue · 3", "Digest", "Trust", "Dashboard", "Background"]} />
          <span style={{ marginLeft: "auto" }}>Main is healthy · watched since 09:00</span>
        </div>
        <div className="cx-t14pg">
          <div className="cx-t10hd">
            <span style={{ color: "var(--color-muted)", fontSize: "12.5px" }}>Dashboard ›</span>
            <b>Reviewer backtest</b>
            <span className="cx-rtag cx-lo">Sample data</span>
            <span className="cx-rtag cx-lo">This device only</span>
            <span className="cx-apchip">Sat 7 Nov</span>
            <span className="cx-t10sp">
              <span className="cx-t10sel"><Ic n="folder" sm />orbyn-engineering<Ic n="down" sm /></span>
              <Btn className="cx-ghost">Back to the report</Btn>
            </span>
          </div>
          <div className="cx-aptb" style={{ gridTemplateColumns: "minmax(0,2.2fr) minmax(0,1.2fr) minmax(0,.8fr) minmax(0,.8fr) minmax(0,.9fr) minmax(0,.8fr)" }}>
            <div className="cx-h">Pull request</div>
            <div className="cx-h">How it escaped</div>
            <div className="cx-h">Claude Code</div>
            <div className="cx-h">Codex</div>
            <div className="cx-h">OpenCode · Security</div>
            <div className="cx-h">Checks</div>
            <div><b>#412</b><small>Rename the legacy returnTo helper</small></div>
            <div><b>Reverted</b><small>Codex · web/</small></div>
            <div><b><Ic n="check2" sm />{" Caught"}</b></div>
            <div><b><Ic n="check2" sm />{" Caught"}</b></div>
            <div><span style={{ color: "var(--color-muted)" }}>Not run</span></div>
            <div><span style={{ color: "var(--color-muted)" }}>Missed</span></div>
            <div><b>#418</b><small>Rotate the signing key</small></div>
            <div><b>Reverted</b><small>Codex · keys/</small></div>
            <div><b><Ic n="check2" sm />{" Caught"}</b></div>
            <div><span style={{ color: "var(--color-muted)" }}>Missed</span></div>
            <div><b><Ic n="check2" sm />{" Caught"}</b></div>
            <div><span style={{ color: "var(--color-muted)" }}>Missed</span></div>
            <div><b>#425</b><small>Revoke the old API tokens</small></div>
            <div><b>Reverted</b><small>Claude Code · auth/</small></div>
            <div><b><Ic n="check2" sm />{" Caught"}</b></div>
            <div><b><Ic n="check2" sm />{" Caught"}</b></div>
            <div><span style={{ color: "var(--color-muted)" }}>Missed</span></div>
            <div><span style={{ color: "var(--color-muted)" }}>Missed</span></div>
            <div><b>#431</b><small>Retry the calendar feed on a timeout</small></div>
            <div><b>Fixed</b><small>Codex · web/</small></div>
            <div><span style={{ color: "var(--color-muted)" }}>Missed</span></div>
            <div><b><Ic n="check2" sm />{" Caught"}</b></div>
            <div><span style={{ color: "var(--color-muted)" }}>Not run</span></div>
            <div><b><Ic n="check2" sm />{" Caught"}</b></div>
            <div><b>#437</b><small>Move the session cookie to SameSite=Lax</small></div>
            <div><b>Reverted</b><small>Codex · auth/</small></div>
            <div><b><Ic n="check2" sm />{" Caught"}</b></div>
            <div><span style={{ color: "var(--color-muted)" }}>Missed</span></div>
            <div><b><Ic n="check2" sm />{" Caught"}</b></div>
            <div><b><Ic n="check2" sm />{" Caught"}</b></div>
            <div><b>#440</b><small>Cache the feed token lookup</small></div>
            <div><b>Fixed</b><small>Claude Code · web/</small></div>
            <div><b><Ic n="check2" sm />{" Caught"}</b></div>
            <div><span style={{ color: "var(--color-muted)" }}>Missed</span></div>
            <div><span style={{ color: "var(--color-muted)" }}>Not run</span></div>
            <div><b><Ic n="check2" sm />{" Caught"}</b></div>
            <div><b>#446</b><small>Batch the digest emails</small></div>
            <div><b>Fixed</b><small>Codex · web/</small></div>
            <div><b><Ic n="check2" sm />{" Caught"}</b></div>
            <div><b><Ic n="check2" sm />{" Caught"}</b></div>
            <div><span style={{ color: "var(--color-muted)" }}>Not run</span></div>
            <div><span style={{ color: "var(--color-muted)" }}>Missed</span></div>
            <div><b>#449</b><small>Tighten the login rate limit</small></div>
            <div><b>Reverted</b><small>Claude Code · auth/</small></div>
            <div><span style={{ color: "var(--color-muted)" }}>Missed</span></div>
            <div><b><Ic n="check2" sm />{" Caught"}</b></div>
            <div><b><Ic n="check2" sm />{" Caught"}</b></div>
            <div><span style={{ color: "var(--color-muted)" }}>Missed</span></div>
            <div><b>#452</b><small>Drop the unused export route</small></div>
            <div><b>Reverted</b><small>Codex · web/</small></div>
            <div><span style={{ color: "var(--color-muted)" }}>Missed</span></div>
            <div><span style={{ color: "var(--color-muted)" }}>Missed</span></div>
            <div><span style={{ color: "var(--color-muted)" }}>Not run</span></div>
            <div><span style={{ color: "var(--color-muted)" }}>Missed</span></div>
          </div>
          <div className="cx-fbx">
            <h6>What the rows say</h6>
            <div className="cx-fkv">
              <span>At least one</span>
              <b>8 of 9 escapes</b>
              <span>None</span>
              <b>#452 · Drop the unused export route</b>
              <span>Two or more</span>
              <b>8 of the 8 that were caught, so no escape depended on a single reviewer or check</b>
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
