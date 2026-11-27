import type { FlowArtifact } from "../../../../generated/FlowArtifact";
import type { FlowCheck } from "../../../../generated/FlowCheck";
import type { FlowTaskArtifacts } from "../../../../generated/FlowTaskArtifacts";
import { Empty, ErrorState, Loading } from "../../../../product/ui/states";
import { Button } from "../../../../ui/controls";
import { checkName, plain, shortCommit } from "./model";
import { useTaskArtifacts } from "./useFlow";
import "./flow-live.css";

/** A field this read does not carry: the chip says Planned and its tooltip names the read that would. */
const Planned = ({ why }: { why: string }) => (
  <span className="fv-label fv-plan" title={why}>Planned</span>
);

/** Why there is no diff: the read's own three fixed reasons, in the person's words. */
const DIFF_ABSENT: Record<string, string> = {
  no_checkout: "This task has no checkout, so there is no change to read.",
  no_base: "The commit this task started from is not recorded, so there is no change to read.",
  unreadable: "The checkout could not be read this time; ask again.",
};

const noDiff = (reason: string | null | undefined): string =>
  (reason ? DIFF_ABSENT[reason] : undefined) ?? "There is no change to read.";

/**
 * The artifacts view of one Flow task (Crew Designs, frame `flow-artifacts`): the checks the build under way ran, the
 * artifacts those rows named and the change the checks read, pinned to the commit the task started from. Each value comes
 * from `GET /api/crew/flow/tasks/{id}/artifacts`; a field the read does not carry stays Planned with the read that would,
 * and a brief, findings or pull request is never filled in from another read. Read only: nothing is written.
 */
export function TaskArtifacts({ id, name, onBack }: { id: string; name?: string; onBack(): void }) {
  const read = useTaskArtifacts(id);
  const missing = read.error?.status === 404;
  return (
    <section className="fv-body fv-zoom" aria-label="Artifacts">
      <div className="fv-crumb">
        <Button small onClick={onBack}>Task</Button>
        <h2 tabIndex={-1} data-page-title>Artifacts</h2>
        {name ? <span className="fv-muted">{name}</span> : null}
      </div>
      {read.status === "loading" ? <Loading label="Reading the task’s artifacts" rows={4} /> : null}
      {read.status === "error" && !read.data ? (
        missing ? (
          <>
            <Empty title="This service has no artifacts read" />
            <p className="fv-muted">The checks, the artifacts they named and the change they read stay Planned.</p>
          </>
        ) : (
          <ErrorState title="Can’t read this task’s artifacts" failure={read.error} onRetry={() => void read.reload()} />
        )
      ) : null}
      {read.data ? <ArtifactsBody read={read.data} /> : null}
    </section>
  );
}

/** The frame's five artifacts and its rules, as far as this read carries them. */
function ArtifactsBody({ read }: { read: FlowTaskArtifacts }) {
  return (
    <>
      {read.truncated ? <p className="fv-note" role="note">A list was cut at this read’s cap; the newest are shown.</p> : null}
      <div className="fv-two">
        <section className="fv-box" aria-label="Brief">
          <h3>Brief</h3>
          <p className="fv-muted">
            The brief and the route are on the task view. <Planned why="The artifacts read carries the checks, the artifacts and the change, never the brief" />
          </p>
        </section>
        <DiffCard read={read} />
        <ChecksCard checks={read.checks} />
        <section className="fv-box" aria-label="Findings">
          <h3>Findings · Code review → Security review</h3>
          <p className="fv-muted">
            The findings are on the task view. <Planned why="The artifacts read carries no findings" />
          </p>
        </section>
        <section className="fv-box" aria-label="Pull request">
          <h3>PR · after the PR opens</h3>
          <p className="fv-muted">
            The Run page carries the pull request a task made. <Planned why="The artifacts read carries no pull request" />
          </p>
        </section>
        <section className="fv-box" aria-label="Rules for every artifact">
          <h3>Rules for every artifact</h3>
          <p>Pinned to a commit. Reviewers are given the diff, the purpose and the check results; not the author’s transcript or verdict.</p>
          <p className="fv-muted">
            Stale when the commit moves on, and marked so. <Planned why="The read reports the checkout’s head and each row’s commit, so a row can be marked stale; this view does not mark one yet" />
          </p>
        </section>
      </div>
    </>
  );
}

/** The change the checks read: the base commit it is pinned to and its counts. Its author, branch and lease are not served. */
function DiffCard({ read }: { read: FlowTaskArtifacts }) {
  const diff = read.diff;
  return (
    <section className="fv-box" aria-label="Diff">
      <h3>Diff · Build → Checks</h3>
      {diff ? (
        <dl>
          <dt>Pinned to</dt>
          <dd><code className="fv-path">{shortCommit(diff.base)}</code></dd>
          <dt>Changed</dt>
          <dd>{diff.files_total} {diff.files_total === 1 ? "file" : "files"} · +{diff.added} −{diff.removed}</dd>
        </dl>
      ) : (
        <p className="fv-muted">{noDiff(read.diff_absent)}</p>
      )}
      <dl>
        <dt>Author</dt>
        <dd><Planned why="The artifacts read carries no author" /></dd>
        <dt>Branch</dt>
        <dd><Planned why="The artifacts read carries no branch" /></dd>
        <dt>Lease</dt>
        <dd><Planned why="The artifacts read carries no file lease" /></dd>
      </dl>
    </section>
  );
}

/** The checks bar: each row by its kind and id (never a check's own text) and the artifacts that row named. */
function ChecksCard({ checks }: { checks: FlowCheck[] }) {
  const commits = [...new Set(checks.map((one) => one.commit).filter((one) => one !== ""))];
  const named = checks.some((one) => one.artifacts.length > 0);
  return (
    <section className="fv-box" aria-label="Check results">
      <h3>Check results · Checks → Code review</h3>
      {checks.length === 0 ? (
        <p className="fv-muted">No check has run for this build yet.</p>
      ) : (
        <>
          <dl>
            <dt>On commit</dt>
            <dd>{commits.length > 0 ? commits.map(shortCommit).join(" · ") : "Not recorded"}</dd>
          </dl>
          <ol className="fv-checks">
            {checks.map((check, at) => (
              <li key={`${check.id}:${at}`}>
                <span className="fv-check-n">Check {at + 1} of {checks.length}</span>
                <span className="fv-check-run"><b>{checkName(check)}</b> · {check.outcome}</span>
                {check.artifacts.length > 0 ? (
                  <ul className="fv-artifacts" aria-label="Artifacts this check named">
                    {check.artifacts.map((artifact, index) => (
                      <li key={`${artifact.hash}:${index}`}><Artifact artifact={artifact} /></li>
                    ))}
                  </ul>
                ) : null}
              </li>
            ))}
          </ol>
        </>
      )}
      {checks.length > 0 && !named ? <p className="fv-muted">No check named an artifact.</p> : null}
      <p className="fv-muted">
        An artifact keeps its hash only; its bytes are not served. <Planned why="Nothing records that an artifact’s bytes were pruned, so a pruned artifact and one never written look the same" />
      </p>
    </section>
  );
}

/** One artifact a check named: its kind, the name its producer wrote when it wrote one, and the hash it is kept by. */
function Artifact({ artifact }: { artifact: FlowArtifact }) {
  return (
    <>
      <span>{artifact.name ? `${plain(artifact.kind)} · ${artifact.name}` : plain(artifact.kind)}</span>
      <code className="fv-path">{artifact.hash}</code>
    </>
  );
}
