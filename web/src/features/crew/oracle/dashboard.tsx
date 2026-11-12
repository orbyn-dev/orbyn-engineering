import { Frame } from "../shared/Frame";
import { Ic, PageHead, Seg } from "../shared/parts";

/** Crew Designs v22, frame `oracle-dashboard`. */
export function OracleDashboard() {
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
            <b>Dashboard</b>
            <span className="cx-rtag cx-lo">Sample data</span>
            <span className="cx-rtag cx-lo">This device only</span>
            <span className="cx-t10sp">
              <span className="cx-t10sel"><Ic n="folder" sm />orbyn-engineering<Ic n="down" sm /></span>
              <Seg kind="cwseg" on={1} items={["30 days", "60 days", "90 days"]} />
            </span>
          </div>
          <div className="cx-t10g cx-t10two" style={{ gridTemplateColumns: "minmax(0,1fr) minmax(0,1fr)" }}>
            <div className="cx-t10c">
              <span className="cx-t10l">Merges with an independent key</span>
              <span className="cx-t10b">24 of 38</span>
              <span className="cx-t10s">written by an engine other than the builder</span>
              <span className="cx-t10f">last 60 days · the other 14: 9 documentation-only, 3 same-engine key you approved, 2 with no second engine signed in</span>
            </div>
            <div className="cx-t10c">
              <span className="cx-t10l">The gate on those 24</span>
              <span className="cx-t10b">17 passed at once</span>
              <span className="cx-t10s">4 after strengthening · 1 without the gate · 2 Not measured</span>
              <span className="cx-t10f">24 keys · last 60 days · 1 without the gate was your choice; 2 Not measured are Go</span>
            </div>
          </div>
          <h5 className="cx-t10h5">By author engine</h5>
          <span className="cx-t10f" style={{ margin: "0", color: "var(--color-muted)", fontSize: "11.5px" }}>
            The builder is a different engine from the author in every row.
          </span>
          <div className="cx-aptb" style={{ gridTemplateColumns: "minmax(0,1.3fr) minmax(0,1.6fr)" }}>
            <div className="cx-h">Author engine</div>
            <div className="cx-h">Keys it wrote</div>
            <div><b>Claude Code · Sonnet 5.5</b><small>Anthropic</small></div>
            <div><b>13 keys</b><small>built by Codex 9, OpenCode 4</small></div>
            <div><b>Codex · GPT-5.5</b><small>OpenAI</small></div>
            <div><b>8 keys</b><small>built by Claude Code 8</small></div>
            <div><b>OpenCode · DeepSeek V4</b><small>DeepSeek</small></div>
            <div><b>3 keys</b><small>built by Claude Code 3</small></div>
          </div>
          <h5 className="cx-t10h5">By language</h5>
          <div className="cx-aptb" style={{ gridTemplateColumns: "minmax(0,1.3fr) minmax(0,.8fr) minmax(0,1.6fr)" }}>
            <div className="cx-h">Language</div>
            <div className="cx-h">Gates run</div>
            <div className="cx-h">What the keys killed</div>
            <div><b>TypeScript</b><small>Stryker</small></div>
            <div><b>16 gates</b></div>
            <div><b>241 of 287</b><small>mutants killed · 84%</small></div>
            <div><b>Rust</b><small>cargo-mutants</small></div>
            <div><b>6 gates</b></div>
            <div><b>58 of 71</b><small>mutants killed · too few gates for a rate</small></div>
            <div><b>Go</b><small>No tool Crew supports yet</small></div>
            <div><b>2 keys</b></div>
            <div><b>Not measured</b><small>no mutants made</small></div>
          </div>
          <div className="cx-fbx">
            <h6>Escapes, and why there is no comparison</h6>
            <div className="cx-fkv">
              <span>With an independent key</span>
              <b>1 escape in 24 merges</b>
              <span>Without</span>
              <b>2 escapes in 14 merges</b>
              <span>Reading it</span>
              <b>Too few to compare. Crew makes no claim that an independent key finds more bugs, and it won’t until it has measured on real repos</b>
            </div>
          </div>
          <div className="cx-fbx">
            <h6>How to read this</h6>
            <p>Counts are exact, for this repo, on this device. A rate shows only from 10 gates. The numbers are sample data.</p>
          </div>
        </div>
      </div>
    </Frame>
  );
}
