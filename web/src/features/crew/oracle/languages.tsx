import { Frame } from "../shared/Frame";
import { Btn } from "../shared/parts";

/** Crew Designs v22, frame `oracle-languages`. */
export function OracleLanguages() {
  return (
    <Frame sheet>
      <div className="cx-cwsheet" style={{ maxWidth: "760px" }}>
        <h3 style={{ margin: "0", fontSize: "16px", fontWeight: 500, color: "var(--color-text)" }}>
          Crew › Independent oracle › Mutation tools
          <span className="cx-cwpv">Preview</span>
        </h3>
        <p style={{ margin: "0", color: "var(--color-muted)", fontSize: "12.5px", lineHeight: 1.5 }}>
          The mutation gate uses the tool of the project’s language. Crew runs a tool that is installed in the project and doesn’t install one for you. A language with no tool, or a tool that isn’t installed, shows Not measured.
        </p>
        <div className="cx-t15plan" style={{ gridTemplateColumns: "minmax(0,1fr) minmax(0,1.3fr) minmax(0,1.2fr)" }}>
          <div className="cx-h">Language</div>
          <div className="cx-h">Mutation tool</div>
          <div className="cx-h">In this project</div>
          <div>Rust</div>
          <div>cargo-mutants</div>
          <div>Not installed</div>
          <div>TypeScript</div>
          <div>Stryker</div>
          <div>Installed · 8.6</div>
          <div>Python</div>
          <div>mutmut</div>
          <div>Not used in this project</div>
          <div>Any other language</div>
          <div>None Crew supports yet</div>
          <div>Shows Not measured</div>
        </div>
        <div className="cx-fbx">
          <h6>Install cargo-mutants for this project</h6>
          <p>The Rust crates in orbyn-engine have no mutation tool yet. Run this in the project, then choose Check again. Crew doesn’t run it for you.</p>
          <div className="cx-t14code">cargo install cargo-mutants</div>
          <div style={{ display: "flex", gap: "8px", marginTop: "6px" }}>
            <Btn className="cx-ghost">Copy the command</Btn>
            <Btn className="cx-btn">Check again</Btn>
          </div>
        </div>
        <div className="cx-fbx">
          <h6>Until then</h6>
          <p>
            A Rust task still gets an answer key written by another engine. The gate says Not measured, and the key is labelled “Not validated”, wherever it is shown.
          </p>
        </div>
        <div className="cx-t13row">
          <span className="cx-t13mk">Sample data</span>
          <span className="cx-t13mk">Proposed: Engineering sets the real values</span>
        </div>
      </div>
    </Frame>
  );
}
