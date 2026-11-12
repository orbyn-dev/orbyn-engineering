import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v21, frame `capacity-sampling`. */
export function CapacitySampling() {
  return (
    <Frame>
      <div className="cx-cwpg">
        <PageHead sub="Login redirect crew · Lead: Fix login redirect on Claude Code · 3 workers" acts />
        <div className="cx-cwtabs">
          <Seg kind="segx" on={1} items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} />
          <span className="cx-end">Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m</span>
        </div>
        <div className="cx-flcr">
          <span style={{ color: "var(--color-muted)" }}>Review queue</span>
          <Ic n="right" sm />
          <b>Batch review · 12 low-risk changes</b>
        </div>
        <div className="cx-cwbn">
          <Ic n="info" sm />
          <span>
            <b>Sampling is on for this repo: 3 of these 12 changes are sampled for your review.</b>
            {" The plan is 1 in 4, rounded up. The other 9 are listed in the digest, and you can review any of them."}
          </span>
          <span className="cx-acts"><Btn className="cx-ghost">Review all 12 instead</Btn><Btn className="cx-btn">Looks good for the 3</Btn></span>
        </div>
        <div style={{ padding: "12px 20px", display: "flex", flexDirection: "column", gap: "12px" }}>
          <div className="cx-t15list">
            <div className="cx-hd"><b>Sampled for review</b><span>3 changes · all checks passed</span></div>
            <div className="cx-t15row">
              <span className="cx-apck"><Ic n="check2" sm /></span>
              <div className="cx-m"><b>Fix a typo in the redirect error</b><small>docs/errors.md · merged 13:48 · Checks 2 of 2</small></div>
              <span><span className="cx-apchip">Sampled for review</span></span>
            </div>
            <div className="cx-t15row">
              <span className="cx-apck"><Ic n="check2" sm /></span>
              <div className="cx-m"><b>Add a test for returnTo removal</b><small>tests/auth/session.test.ts · merged 12:30 · Checks 4 of 4</small></div>
              <span><span className="cx-apchip">Sampled for review</span></span>
            </div>
            <div className="cx-t15row">
              <span className="cx-apck"><Ic n="check2" sm /></span>
              <div className="cx-m"><b>Update the Planner API example</b><small>docs/api.md · merged 11:05 · Checks 2 of 2</small></div>
              <span><span className="cx-apchip">Sampled for review</span></span>
            </div>
          </div>
          <div className="cx-t15list">
            <div className="cx-hd"><b>Not sampled</b><span>9 changes · 6 shown · all checks passed</span></div>
            <div className="cx-t15row">
              <span className="cx-apck cx-off" />
              <div className="cx-m"><b>Update the login docs</b><small>docs/login.md · merged 14:12 · Checks 2 of 2</small></div>
              <span><span className="cx-t15hold"><Ic n="info" sm />Not sampled · listed in the digest</span></span>
            </div>
            <div className="cx-t15row">
              <span className="cx-apck cx-off" />
              <div className="cx-m"><b>Rename a test helper</b><small>tests/helpers.ts · merged 10:22 · Checks 3 of 3</small></div>
              <span><span className="cx-t15hold"><Ic n="info" sm />Not sampled · listed in the digest</span></span>
            </div>
            <div className="cx-t15row">
              <span className="cx-apck cx-off" />
              <div className="cx-m"><b>Fix a broken link in the README</b><small>README.md · merged 09:12 · Checks 1 of 1</small></div>
              <span><span className="cx-t15hold"><Ic n="info" sm />Not sampled · listed in the digest</span></span>
            </div>
            <div className="cx-t15row">
              <span className="cx-apck cx-off" />
              <div className="cx-m"><b>Update the docs index</b><small>docs/index.md · merged Fri 6 Nov, 17:40 · Checks 2 of 2</small></div>
              <span><span className="cx-t15hold"><Ic n="info" sm />Not sampled · listed in the digest</span></span>
            </div>
            <div className="cx-t15row">
              <span className="cx-apck cx-off" />
              <div className="cx-m"><b>Fix a typo in the feed docs</b><small>docs/feed.md · merged Fri 6 Nov, 16:55 · Checks 2 of 2</small></div>
              <span><span className="cx-t15hold"><Ic n="info" sm />Not sampled · listed in the digest</span></span>
            </div>
            <div className="cx-t15row">
              <span className="cx-apck cx-off" />
              <div className="cx-m"><b>Add a snapshot test for the feed</b><small>tests/feed.test.ts · merged Fri 6 Nov, 15:20 · Checks 3 of 3</small></div>
              <span><span className="cx-t15hold"><Ic n="info" sm />Not sampled · listed in the digest</span></span>
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
