import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v22, frame `oracle-report`. */
export function OracleReport() {
  return (
    <Frame>
      <div className="cx-cwpg">
        <PageHead sub="Login redirect crew · Lead: Fix login redirect on Claude Code · 3 workers" acts />
        <div className="cx-cwtabs">
          <Seg kind="segx" on={6} items={["Flow", "Review · 7", "Map", "Board", "Lanes", "Inbox · 4", "Report"]} />
          <span className="cx-end">Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m</span>
        </div>
        <div className="cx-flcr">
          <span style={{ color: "var(--color-muted)" }}>All tasks</span>
          <Ic n="right" sm />
          <span style={{ color: "var(--color-muted)" }}>Add a rate limit to the login route</span>
          <Ic n="right" sm />
          <b>Report</b>
          <span className="cx-end"><span>Commit 5d2e1a8 · crew/login-limit</span></span>
        </div>
        <div style={{ padding: "12px 20px 0", flex: "none" }}>
          <div className="cx-frh">
            <b>Add a rate limit to the login route</b>
            <span className="cx-st cx-ok"><i className="cx-sd" />Passed · gate passed</span>
          </div>
        </div>
        <div className="cx-rpg">
          <div className="cx-pm">
            <div className="cx-cllist">
              <div className="cx-lh">
                <b>Claims</b>
                <span>5 claims · 4 verified · 1 unverified</span>
                <span className="cx-le">
                  <Btn className="cx-ghost" style={{ height: "26px", fontSize: "12px" }}><Ic n="refresh" sm />Run all again</Btn>
                  <Btn className="cx-btn" style={{ height: "26px", fontSize: "12px" }}><Ic n="plus" sm />Add a check</Btn>
                </span>
              </div>
              <div className="cx-clm">
                <span className="cx-cmi cx-cok"><Ic n="ok" sm /></span>
                <div className="cx-cmt">
                  <b>Five failed logins in one minute lock the route for the rest of the minute.</b>
                  <span className="cx-cme">Answer key check 1 · Claude Code · Sonnet 5.5 · exit 0 · 5d2e1a8</span>
                </div>
                <div className="cx-cms"><span className="cx-st cx-ok"><i className="cx-sd" />Passed</span></div>
                <span className="cx-cmp"><Ic n="right" sm /></span>
              </div>
              <div className="cx-clm">
                <span className="cx-cmi cx-cok"><Ic n="ok" sm /></span>
                <div className="cx-cmt">
                  <b>The sixth request in a minute gets 429 with a Retry-After header.</b>
                  <span className="cx-cme">Answer key check 2 · Claude Code · Sonnet 5.5 · exit 0 · 5d2e1a8</span>
                </div>
                <div className="cx-cms"><span className="cx-st cx-ok"><i className="cx-sd" />Passed</span></div>
                <span className="cx-cmp"><Ic n="right" sm /></span>
              </div>
              <div className="cx-clm">
                <span className="cx-cmi cx-cok"><Ic n="ok" sm /></span>
                <div className="cx-cmt">
                  <b>A different address is not limited.</b>
                  <span className="cx-cme">Answer key check 4 · Claude Code · Sonnet 5.5 · exit 0 · 5d2e1a8</span>
                </div>
                <div className="cx-cms"><span className="cx-st cx-ok"><i className="cx-sd" />Passed</span></div>
                <span className="cx-cmp"><Ic n="right" sm /></span>
              </div>
              <div className="cx-clm">
                <span className="cx-cmi cx-cok"><Ic n="ok" sm /></span>
                <div className="cx-cmt">
                  <b>The change touches just auth/login.ts, auth/rate-limit.ts and their tests.</b>
                  <span className="cx-cme">File scope · 3 files, all leased · 5d2e1a8</span>
                </div>
                <div className="cx-cms"><span className="cx-st cx-ok"><i className="cx-sd" />Passed</span></div>
                <span className="cx-cmp"><Ic n="right" sm /></span>
              </div>
              <div className="cx-clm cx-cunv">
                <span className="cx-cmi cx-cunv" />
                <div className="cx-cmt">
                  <b>The limit doesn’t slow down a normal login.</b>
                  <span className="cx-cmn">No check backs this. It is a statement by the worker, not a result.</span>
                </div>
                <div className="cx-cms"><span className="cx-rtag cx-lo">Unverified</span></div>
                <span className="cx-cmp" />
              </div>
            </div>
          </div>
          <div className="cx-ps">
            <div className="cx-nvc">
              <h6><Ic n="info" sm />Not verified</h6>
              <ul>
                <li>Whether a normal login is slower (a claim with no check)</li>
                <li>Behaviour behind a proxy that hides the address</li>
                <li>Many addresses at once</li>
                <li>The 3 mutants that survived: see the answer key</li>
              </ul>
              <span className="cx-cmn">Nothing here ran or was checked. “Passed” means the checks listed on the left passed, and nothing else.</span>
            </div>
            <div className="cx-nvc">
              <h6><Ic n="eye" sm />{"Model reviews "}<span className="cx-rtag">Advisory</span></h6>
              <div className="cx-prf"><span>Claude Code</span><b>Correctness · 0 findings</b><span>OpenCode</span><b>Performance · 1 unconfirmed finding</b></div>
              <span className="cx-cmn">A review by a model is a finding to look at. It never passes or fails a task: checks and your decision do.</span>
            </div>
            <div className="cx-nvc">
              <h6><Ic n="shield" sm />{"Answer key "}<span className="cx-rtag">Independent</span></h6>
              <div className="cx-prf">
                <span>Written by</span>
                <b>Claude Code · Sonnet 5.5</b>
                <span>Built by</span>
                <b>Codex · GPT-5.5</b>
                <span>Given to the author</span>
                <b>The brief and the contract facts</b>
                <span>Mutation gate</span>
                <b><span className="cx-st cx-ok"><i className="cx-sd" />Passed</span>{" 15 of 18 killed · 3 survived"}</b>
              </div>
              <span className="cx-cmn">
                {"Written by another engine that did not see the code. The gate shows the checks can catch a break; it doesn’t show that the change is right. "}
                <u>Open the answer key</u>
              </span>
            </div>
          </div>
        </div>
      </div>
    </Frame>
  );
}
