import { Frame } from "../shared/Frame";
import { Btn, PageHead, Seg } from "../shared/parts";
import { Ic } from "./icons";

/** Crew Designs, frame `flow-artifacts`: Flow: the five artifacts on the lines. */
export function FlowArtifacts() {
  return (
    <Frame height={700}>
      <div className="cx-cwpg">
            <PageHead sub="Login redirect crew · Task: Use next in the auth redirect · 5 runs" acts />
            <div className="cx-cwtabs">
              <Seg kind="segx" items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} on={0} />
              <span className="cx-end">Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m</span>
            </div>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(3,minmax(0,1fr))", gap: "16px", padding: "20px 28px", alignItems: "start", justifyItems: "start" }}>
              <div className="cx-fpop" style={{ width: "300px" }}>
                <h6><Ic n="brief" sm />Brief</h6>
                <div className="cx-fkv">
                  <span>Written by</span>
                  <b>Lead · Opus 5.5</b>
                  <span>Purpose</span>
                  <b>Use next when it points inside the app</b>
                  <span>Scope</span>
                  <b>auth/redirect.ts</b>
                  <span>Answer key</span>
                  <b>3 checks, no limit</b>
                  <span>Route</span>
                  <b>Claude Code · Sonnet 5.5</b>
                </div>
                <div className="cx-ra"><Btn className="cx-btn">Open the brief</Btn><Btn className="cx-ghost">Add a check</Btn></div>
              </div>
              <div className="cx-fpop" style={{ width: "300px" }}>
                <h6><Ic n="diff" sm />Diff · Build → Checks</h6>
                <div className="cx-fkv">
                  <span>Commit</span>
                  <b>a41c9e2</b>
                  <span>Changed</span>
                  <b>2 files · +18 −4</b>
                  <span>Author</span>
                  <b>Claude Code · Sonnet 5.5</b>
                  <span>Branch</span>
                  <b>crew/redirect-impl</b>
                  <span>Lease</span>
                  <b>auth/redirect.ts</b>
                </div>
                <div className="cx-ra"><Btn className="cx-btn">Open the diff</Btn><Btn className="cx-ghost">Copy commit</Btn></div>
              </div>
              <div className="cx-fpop" style={{ width: "320px" }}>
                <h6><Ic n="check2" sm />Check results · Checks → Code review</h6>
                <div className="cx-fkv">
                  <span>On commit</span>
                  <b>a41c9e2 · current</b>
                  <span>npm test auth</span>
                  <b>exit 0 · 21 s</b>
                  <span>tsc --noEmit</span>
                  <b>exit 0 · 14 s</b>
                  <span>File scope</span>
                  <b>exit 0 · just auth/redirect.ts</b>
                  <span>Proves</span>
                  <b>The listed tests pass. Not that other callers work.</b>
                </div>
                <div className="cx-ra"><Btn className="cx-btn">Open results</Btn><Btn className="cx-ghost">Re-run all</Btn></div>
              </div>
              <div className="cx-fpop" style={{ width: "320px" }}>
                <h6><Ic n="bug" sm />Findings · Code review → Security review</h6>
                <div className="cx-fkv">
                  <span>Confirmed</span>
                  <b>0 (these block)</b>
                  <span>Unconfirmed</span>
                  <b>1 · Docs · low</b>
                  <span>Dropped</span>
                  <b>0</b>
                  <span>Latest</span>
                  <b>auth/redirect.test.ts:12 · the example uses //evil.com</b>
                </div>
                <div className="cx-ra"><Btn className="cx-btn">Open findings</Btn><Btn className="cx-ghost">Run the Finding check</Btn></div>
              </div>
              <div className="cx-fpop" style={{ width: "300px" }}>
                <h6><Ic n="pr" sm />PR · after the PR opens<span className="cx-cwpv">Planned</span></h6>
                <div className="cx-fkv">
                  <span>Pull request</span>
                  <b>#482 · draft</b>
                  <span>Branch</span>
                  <b>crew/redirect-impl → fix/login</b>
                  <span>CI</span>
                  <b>2 of 2 required checks passed</b>
                  <span>PR review</span>
                  <b>2 lanes · comments link back here</b>
                </div>
                <div className="cx-ra"><Btn className="cx-btn">Open the PR</Btn><Btn className="cx-ghost">Copy link</Btn></div>
              </div>
              <div className="cx-fbx" style={{ width: "300px" }}>
                <h6>Rules for every artifact</h6>
                <p>Pinned to a commit. Stale when the commit moves on, and marked so. Reviewers are given the diff, the purpose and the check results; not the author’s transcript or verdict.</p>
              </div>
            </div>
      </div>
    </Frame>
  );
}
