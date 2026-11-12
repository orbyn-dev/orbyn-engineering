import { Frame } from "../shared/Frame";
import { PageHead, Seg } from "../shared/parts";

/** Crew Designs v22, frame `oracle-narrow-window`. */
export function OracleNarrowWindow() {
  return (
    <Frame width={640}>
      <div className="cx-cwpg">
        <PageHead sub="Login redirect crew" />
        <div className="cx-cwtabs" style={{ padding: "8px 14px", overflow: "hidden" }}>
          <Seg kind="segx" on={6} items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} />
        </div>
        <div style={{ padding: "10px 14px", display: "flex", flexDirection: "column", gap: "10px", overflow: "hidden", minHeight: "0" }}>
          <div className="cx-t10hd"><b>Answer key</b><span className="cx-rtag cx-lo">Sample data</span></div>
          <div className="cx-t14card">
            <b>Add a rate limit to the login route</b>
            <div className="cx-t14r"><span className="cx-apchip">Independent</span><span className="cx-st cx-ok"><i className="cx-sd" />Gate passed</span></div>
            <small>Built by Codex · GPT-5.5</small>
            <small>Key by Claude Code · Sonnet 5.5: another engine, another vendor</small>
            <small>Given only the brief and the contract facts</small>
          </div>
          <div className="cx-t14card">
            <b>Mutation gate · Stryker</b>
            <small>15 of 18 mutants killed · 3 survived</small>
            <small>The gate fails only when no mutant is killed.</small>
          </div>
          <div className="cx-t14card">
            <b>m7</b>
            <small>auth/rate-limit.ts:41</small>
            <small>{"count > limit  →  count >= limit"}</small>
            <small>No check sends exactly five requests, so the edge of the limit is not tested.</small>
          </div>
          <div className="cx-t14card">
            <b>m12</b>
            <small>auth/rate-limit.ts:58</small>
            <small>{"resetAt = now + 60_000  →  removed"}</small>
            <small>The “starts again” check waits out the minute with a fake clock that the mutant does not touch.</small>
          </div>
          <div className="cx-t14card">
            <b>m16</b>
            <small>auth/login.ts:33</small>
            <small>{"Retry-After: 60  →  Retry-After: 61"}</small>
            <small>The check asserts that the header is there, not its value.</small>
          </div>
        </div>
      </div>
    </Frame>
  );
}
