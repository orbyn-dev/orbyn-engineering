import { ErrorState, Loading } from "../../../../product/ui/states";
import { useCrewPreview } from "../../autopilot/approvals/useCrew";
import { Frame } from "../../shared/Frame";
import { useCrewHref } from "../../shared/nav";
import { Ic } from "../../shared/parts";
import { Planned } from "../../capacity/Planned";
import { WHY } from "./model";
import { useEscapeThresholds } from "./useEscapes";

const suffix = (stored: boolean) => (stored ? "" : " · default");

/**
 * Crew Designs v79, `escape-settings`, on the real thresholds read. The four constants the log reads are drawn from
 * `GET /api/crew/escapes/thresholds` exactly as the device stores them (or, when nobody stored values, the defaults,
 * each row saying "· default"). Read only: changing them is a person's PUT that is not built, so every editor control
 * stays a drawn value beside its Planned pill, and nothing on this screen sends a write.
 */
export function LiveEscapeSettings() {
  const on = useCrewPreview();
  // No route is read while the service's Crew Preview is off: the read sits in the body, mounted only here.
  if (!on) return null;
  return <SettingsBody />;
}

function SettingsBody() {
  const read = useEscapeThresholds();
  const href = useCrewHref();
  return (
    <Frame sheet live>
      <div className="cx-cwsheet" style={{ maxWidth: "760px" }}>
        <div style={{ display: "flex" }}>
          <a
            className="cx-t10r"
            href={href({ page: "escapes" })}
            style={{ marginLeft: "0", whiteSpace: "nowrap", display: "inline-flex", alignItems: "center", gap: "2px", color: "inherit" }}
          >
            <Ic n="left" sm />
            <span style={{ textDecoration: "underline", textUnderlineOffset: "2px" }}>Back to escapes</span>
          </a>
        </div>
        <h3 style={{ margin: "0", fontSize: "16px", fontWeight: 500, color: "var(--color-text)" }}>
          Crew › Escapes
          <span className="cx-cwpv">Preview</span>
        </h3>
        <p style={{ margin: "0", color: "var(--color-muted)", fontSize: "12.5px", lineHeight: 1.5 }}>
          Every revert, and every bug found after a merge that you don’t revert, is a possible escape. You confirm it, and
          from a confirmed escape you can ask Crew to propose a check: one new test file, never a policy file. The check is
          a normal task: it goes through review and the merge queue like any change, and nothing reaches your repo before
          it merges.
        </p>
        {read.status === "loading" ? <Loading label="Reading the escape constants" rows={3} /> : null}
        {read.status === "error" ? <ErrorState title="Can’t read the escape constants" failure={read.error} onRetry={() => void read.reload()} /> : null}
        {read.status === "ready" ? <Constants {...read.data} /> : null}
        <p style={{ margin: "2px 0 0", display: "flex", gap: "6px", alignItems: "flex-start", color: "var(--color-muted)", fontSize: "12px", lineHeight: 1.45 }}>
          <Ic n="info" sm />
          <span>Workers run as you, with your machine’s permissions.</span>
        </p>
      </div>
    </Frame>
  );
}

function Constants({ report_days, follow_up_days, min_merges, rate_multiple, stored }: { report_days: number; follow_up_days: number; min_merges: number; rate_multiple: number; stored: boolean }) {
  return (
    <>
      <div>
        <div className="cx-setrow">
          <div>
            <b>
              Propose the check <Planned why={WHY.openItself} style={{ marginLeft: "6px" }} />
            </b>
            <small>
              On a confirmed escape you can ask Crew to propose a check. Nothing runs until you accept it, under every mode
              including Full: that is fixed in v1. “Open it by itself” is Planned.
            </small>
          </div>
          <span className="cx-sv">Ask me each time · default</span>
        </div>
        <div className="cx-setrow">
          <div>
            <b>Kind of gap</b>
            <small>Crew suggests a test gap or a tooling gap. You set it on a possible or confirmed escape. A check is proposed for a test gap only; a tooling gap is logged with no proposal.</small>
          </div>
          <span className="cx-sv cx-ro">Suggested by Crew</span>
        </div>
        <div className="cx-setrow">
          <div>
            <b>Proof before a check is added</b>
            <small>A check is added only when it fails on the escaped merge and passes on the fix or revert. If it passes on both it is marked failed and you decide. Where this machine can’t confine the proof, it waits for your approval of the exact change.</small>
          </div>
          <span className="cx-sv cx-ro">Required</span>
        </div>
        <div className="cx-setrow">
          <div>
            <b>Counted against</b>
            <small>Each escape counts against the engine and the area (the first two folders of a path) of the merge, and the model once the merge records one. Display only: nothing reads the counts.</small>
          </div>
          <span className="cx-sv cx-ro">Engine · area · model when recorded</span>
        </div>
        <div className="cx-setrow">
          <div>
            <b>
              Router tie-break <Planned why={WHY.router} style={{ marginLeft: "6px" }} />
            </b>
            <small>In v1 escape counts decide nothing: confirmed escapes do not feed the Router. A tie-break is Planned.</small>
          </div>
          <span className="cx-sv cx-ro">Not used in v1 · Proposed</span>
        </div>
        <div className="cx-setrow">
          <div>
            <b>Window</b>
            <small>How far back the log and the counts go. Read from this device{stored ? "" : ": these are the defaults, nobody stored other values"}.</small>
          </div>
          <span className="cx-sv cx-ro">{`${report_days} days${suffix(stored)}`}</span>
        </div>
        <div className="cx-setrow">
          <div>
            <b>Later fix</b>
            <small>How long after a merge a “fix” commit counts as a possible escape.</small>
          </div>
          <span className="cx-sv cx-ro">{`${follow_up_days} days${suffix(stored)}`}</span>
        </div>
        <div className="cx-setrow">
          <div>
            <b>Merges before a rate</b>
            <small>
              A group needs this many merges before a rate is shown. No rate is shown in v1 <Planned why={WHY.report} style={{ marginLeft: "6px" }} />.
            </small>
          </div>
          <span className="cx-sv cx-ro">{`${min_merges} merges${suffix(stored)}`}</span>
        </div>
        <div className="cx-setrow">
          <div>
            <b>
              Rate multiple <Planned why={WHY.put} style={{ marginLeft: "6px" }} />
            </b>
            <small>How many times the baseline a rate must be to be called out. Changing any of these is Planned.</small>
          </div>
          <span className="cx-sv cx-ro">{`${rate_multiple}×${suffix(stored)}`}</span>
        </div>
      </div>
      <div className="cx-fbx">
        <h6>What counts</h6>
        <div className="cx-fkv">
          <span>A possible escape</span>
          <b>A revert, at once; a later fix on a line the merge added; or a repository check that failed on this device after the merge. Crew finds it, you decide. Counted apart</b>
          <span>A confirmed escape</span>
          <b>You confirmed it, or you marked the merge. Final: no undo in v1</b>
          <span>Checks</span>
          <b>This device’s own repository checks; a forge’s CI status is Planned</b>
          <span>Cost</span>
          <b>Not part of the record</b>
        </div>
      </div>
    </>
  );
}
