import { useMemo, useState } from "react";
import { ApiFailure } from "../../../api/client";
import type { FlowTaskDetail } from "../../../generated/FlowTaskDetail";
import { useUi } from "../../../product/state/ui";
import { Empty, ErrorState, Loading } from "../../../product/ui/states";
import { Button } from "../../../ui/controls";
import { useFlowTask, useRunPage } from "../flow/live/useFlow";
import { saveTextFile } from "./download";
import { fileNameOf, runPageHtml, runPageMarkdown, runPageModel } from "./runExport";
import "./runpage.css";

type Copy = { kind: "copied"; at: string } | { kind: "failed" } | null;

const Planned = ({ why }: { why: string }) => (
  <span className="rp-chip rp-planned" title={why}>Planned</span>
);

/**
 * The Run page of one Crew task, built on this device from its Flow read and, when the service has it, the task's run read.
 * Save as file hands the person a static page and Copy PR comment puts Markdown on the clipboard; each is one click and
 * nothing is sent anywhere. A 404 run read means this service has none, so the page keeps the Flow-based Planned marks; any
 * other failure is reported beside the page without dropping it. What neither read carries (the plan, the pull request
 * URL and head, the signed proof, another device) stays Planned, never filled in.
 */
export function RunPage({ detail, pull = null, onBack }: { detail: FlowTaskDetail; pull?: number | null; onBack?(): void }) {
  const pushToast = useUi((state) => state.pushToast);
  const runRead = useRunPage(detail.row.id);
  const noRunRead = runRead.error instanceof ApiFailure && runRead.error.status === 404;
  const runFailure = runRead.error && !noRunRead ? runRead.error : null;
  const run = runRead.data ?? undefined;
  const model = useMemo(() => runPageModel(detail, new Date().toISOString(), run), [detail, run]);
  const [copy, setCopy] = useState<Copy>(null);
  const save = () => {
    const made = runPageModel(detail, new Date().toISOString(), run);
    saveTextFile(fileNameOf(made.title), runPageHtml(made));
    pushToast("Download started");
  };
  const copyComment = async () => {
    const text = runPageMarkdown(runPageModel(detail, new Date().toISOString(), run), pull);
    try {
      await navigator.clipboard.writeText(text);
      setCopy({ kind: "copied", at: new Date().toISOString().slice(0, 16).replace("T", " ") + " UTC" });
    } catch {
      setCopy({ kind: "failed" });
      pushToast("Couldn’t copy. The clipboard is unavailable.");
    }
  };
  return (
    <section className="rp" aria-label="Run page">
      <div className="rp-head">
        {onBack ? <Button small onClick={onBack}>Task</Button> : null}
        <h2 tabIndex={-1} data-page-title>{model.title}</h2>
        <span className={`rp-chip ${model.tone === "fail" ? "rp-fail" : ""}`}>{model.stateWord}</span>
        <span className="rp-muted">Crew {model.crew} · round {model.round} · {model.madeAt}</span>
      </div>
      <div className="rp-actions">
        <span className="rp-muted">On this device. Nothing is uploaded.</span>
        <span className="rp-spacer" />
        <Button small onClick={save}>Save as file</Button>
        <Button small onClick={() => void copyComment()}>Copy PR comment</Button>
      </div>
      {runFailure ? (
        <div className="rp-sec">
          <ErrorState compact title="Can’t read this run’s details" failure={runFailure} onRetry={() => void runRead.reload()} />
        </div>
      ) : null}
      {copy ? (
        <p className="rp-status" role="status">
          {copy.kind === "copied"
            ? `Copied to the clipboard on ${copy.at}. Paste it into a comment on ${pull != null ? `pull request #${pull}` : "the pull request"} yourself.`
            : "Couldn’t copy. The clipboard is unavailable."}
        </p>
      ) : null}
      {model.tone === "fail" ? <p className="rp-banner" role="alert">{model.stateWord}: this run did not pass. Check the checks and the findings before relying on it.</p> : null}

      <div className="rp-sec">
        <h3>Brief</h3>
        <p>{model.purpose}</p>
        <dl>
          <dt>Files</dt><dd>{model.files.length === 0 ? "None" : model.files.join(", ")}</dd>
          <dt>Budget</dt><dd>{model.budget}</dd>
          <dt>Route</dt><dd>{model.route ?? "Not routed yet"}</dd>
        </dl>
        {model.hasRun ? (
          model.prompt ? (
            <>
              <h4>Prompt</h4>
              <p>{model.prompt.text}</p>
              <p className="rp-muted">{model.prompt.note}</p>
            </>
          ) : <p className="rp-muted">No prompt was saved for this task.</p>
        ) : null}
        <p className="rp-muted">
          {model.hasRun ? "Plan" : "Prompt and plan"}{" "}
          <Planned why={model.hasRun ? "Nothing stores a task's plan steps" : "The Flow read carries the brief's purpose only"} />
        </p>
      </div>

      <div className="rp-sec">
        <h3>Checks</h3>
        <p className={model.checksFailed ? "rp-fail" : undefined}>{model.checks ?? "No check result yet."}</p>
        {model.checkList ? (
          model.checkList.length === 0
            ? <p className="rp-muted">No checks were recorded for this run.</p>
            : (
              <ul>
                {model.checkList.map((one, at) => (
                  <li key={at}>
                    <b>{one.outcome}</b> <code>{one.command}</code>
                    <div className="rp-muted">
                      {one.exitCode != null ? <span>exit {one.exitCode}</span> : <span>no exit code</span>}
                      {one.duration ? <> · <span>{one.duration}</span></> : null}
                      {one.commit ? <> · <span>commit {one.commit}</span></> : null}
                    </div>
                    <div className="rp-muted">Proves: <span>{one.proves}</span></div>
                  </li>
                ))}
              </ul>
            )
        ) : (
          <p className="rp-muted">Each check's command, exit code, duration and what it proves <Planned why="The Flow read carries counts, never a check" /></p>
        )}
        {model.runTruncated ? <p className="rp-muted">A list of this run was cut at its cap.</p> : null}
      </div>

      {model.notVerified ? (
        <div className="rp-sec">
          <h3>Not verified</h3>
          {model.notVerified.length === 0
            ? <p className="rp-muted">Nothing was recorded as not verified.</p>
            : <ul>{model.notVerified.map((line, at) => <li key={at}>{line}</li>)}</ul>}
        </div>
      ) : null}

      {model.hasRun ? (
        <div className="rp-sec">
          <h3>Commit and pull request</h3>
          <dl>
            <dt>Commit</dt><dd>{model.commit ?? "No commit recorded."}</dd>
            <dt>Pull request</dt>
            <dd>
              {model.pull ? (
                <>
                  {model.pull.number != null ? <span>#{model.pull.number}</span> : <span>Not numbered yet</span>}
                  {model.pull.branch ? <> · <code>{model.pull.branch}</code></> : null}
                  {model.pull.commit ? <> · <code>{model.pull.commit}</code></> : null}
                </>
              ) : "This task made no pull request."}
            </dd>
          </dl>
          <p className="rp-muted">Pull request URL and head (the stale proof) <Planned why="The read has no pull request URL or head" /></p>
        </div>
      ) : null}

      <div className="rp-sec">
        <h3>Findings</h3>
        {model.findings.length === 0 ? <p className="rp-muted">No findings.</p> : (
          <ul>
            {model.findings.map((one, at) => (
              <li key={at}>
                <b>{one.claim}</b> <code>{one.where}</code>
                <div className="rp-muted">{one.who} · {one.severity} · {one.check} · {one.status}{one.blocks ? " · blocks" : ""}</div>
              </li>
            ))}
          </ul>
        )}
        {model.findingsOmitted > 0 ? <p className="rp-muted">{model.findingsOmitted} more findings are not listed.</p> : null}
        <p className="rp-muted">Only confirmed findings block. Model reviews are advisory.</p>
      </div>

      {model.gates.length > 0 ? (
        <div className="rp-sec"><h3>Gates</h3><ul>{model.gates.map((gate, at) => <li key={at}>{gate}</li>)}</ul></div>
      ) : null}
      {model.merge ? <div className="rp-sec"><h3>Merge</h3><p>{model.merge}</p></div> : null}

      <div className="rp-sec">
        <h3>Signed proof <Planned why="Signing the result is not in the beta" /></h3>
        <p className="rp-muted">Signing the result and verifying it are not in the beta. This page is a snapshot of this device’s records.</p>
        <div><Button small disabled title="Planned: verifying is not in the beta">Verify <span className="rp-planned">(Planned)</span></Button></div>
        <p className="rp-muted">Opening this page from another device <Planned why="The page is built from this device's records" /></p>
      </div>
    </section>
  );
}

/** The Run page of a task by id: reads the task, then draws it. */
export function RunTab({ task, pull, onBack }: { task: string; pull: number | null; onBack?(): void }) {
  const read = useFlowTask(task);
  if (read.status === "loading") return <Loading label="Reading the run" rows={4} />;
  if (!read.data) {
    return read.error instanceof ApiFailure && read.error.status === 404
      ? <Empty title="This run isn’t in Flow" />
      : <ErrorState title="Can’t read this run" failure={read.error} onRetry={() => void read.reload()} />;
  }
  return <RunPage detail={read.data} pull={pull} onBack={onBack} />;
}
