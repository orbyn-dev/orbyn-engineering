import { ErrorState, Loading } from "../../../../product/ui/states";
import { NOT_CONNECTED } from "./api";
import { GUARD_WORKERS } from "./copy";
import { Chip } from "./parts";
import { useAutopilotGuards } from "./useCrew";

const DESTRUCTIVE: [string, string][] = [
  ["Data", "DROP, TRUNCATE, DELETE without a WHERE"],
  ["Cloud and infrastructure", "Deleting a bucket, a volume, an instance, a snapshot or a backup; terraform destroy"],
  ["Kubernetes", "Deleting a namespace or a persistent volume claim"],
  ["Files", "rm -rf outside the worktree"],
  ["Registries", "Unpublishing a package or an image"],
  ["Secrets", "Revoking a key or a token"],
];

function Targets({ title }: { title: string }) {
  return (
    <div className="ap-targets" role="table" aria-label={title}>
      <div className="ap-target-row ap-head" role="row">
        <span role="columnheader">{title}</span>
        <span role="columnheader">Matched by</span>
        <span role="columnheader">Source</span>
      </div>
      <p className="ap-muted ap-note" role="note">{NOT_CONNECTED}: the repo’s target lists are not served yet.</p>
    </div>
  );
}

/** Settings › Crew › Production-linked targets: a hard rule, read-only, with the targets it guards. */
export function ProductionTargets() {
  const read = useAutopilotGuards();
  if (read.status === "loading") return <Loading label="Reading the guard" rows={3} />;
  if (read.status === "error") return <ErrorState title="Can't read the guard" failure={read.error} onRetry={read.reload} />;
  const secondKeys = read.data.second_key_engines;
  return (
    <section className="ap-set" aria-labelledby="ap-guard-title">
      <header className="ap-set-head">
        <h2 id="ap-guard-title">Production-linked targets</h2>
        <Chip tone="plain">Preview</Chip>
      </header>
      <section className="ap-box">
        <h4>Always on, in every mode</h4>
        <p>A destructive command against a production-linked target needs two keys from models under Full: the lead’s and one from a different vendor. Under On and Off it is held for you. You can always approve a command yourself, once, as one deliberate action that names the exact command and the target; that makes you the final authority, not a second key, and it is logged. This is a hard rule, checked by Orbyn’s code before any model sees the request. Read-only: it can’t be switched off here, and nothing the lead decides can either.</p>
      </section>
      <section className="ap-box">
        <h4>What is production-linked</h4>
        <p>Remote targets are guarded unless listed as safe. A remote database, a cloud account or project, a Kubernetes context, a Terraform workspace or a bucket is guarded, and an unknown remote is guarded. Local targets are not. Targets are matched against the host names, profile names, project ids, context names and workspace names found in the command, its environment and the project’s config files.</p>
      </section>
      <Targets title="Always guarded" />
      <Targets title="Exempt (safe)" />
      <section className="ap-box">
        <h4>What counts as destructive</h4>
        <p>A built-in list by category, plus patterns a repo can add in .orbyn/crew.json. These are examples; the list isn’t complete. Orbyn checks the command, not the worker’s description of it.</p>
        <div className="ap-targets" role="table" aria-label="What counts as destructive">
          <div className="ap-destroy-row ap-head" role="row">
            <span role="columnheader">Category (examples)</span>
            <span role="columnheader">Commands that count, for example</span>
          </div>
          {DESTRUCTIVE.map(([category, example]) => (
            <div key={category} className="ap-destroy-row" role="row">
              <span role="cell"><b>{category}</b></span>
              <span role="cell">{example}</span>
            </div>
          ))}
        </div>
      </section>
      <section className="ap-box">
        <h4>The second key</h4>
        <p>
          It comes from an engine on a different vendor from the lead’s.{" "}
          {secondKeys.length ? `Engines that can be a second key: ${secondKeys.join(" and ")}. ` : ""}
          With none signed in, a guarded command is denied for now, and you can approve it yourself.
        </p>
      </section>
      <p className="ap-muted">{GUARD_WORKERS}</p>
    </section>
  );
}
