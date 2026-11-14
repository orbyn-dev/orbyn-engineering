import type { GuardList } from "../../../../generated/GuardList";
import type { GuardTarget } from "../../../../generated/GuardTarget";
import type { GuardTargetKind } from "../../../../generated/GuardTargetKind";
import { ErrorState, Loading } from "../../../../product/ui/states";
import { GUARD_WORKERS } from "./copy";
import { Chip } from "./parts";
import { useAutopilotGuards, useGuardTargets } from "./useCrew";

/** What the guard counts as destructive, by category, as the designs list them. The patterns are built in: no repository file changes them. */
const DESTRUCTIVE: [string, string][] = [
  ["Data", "DROP, TRUNCATE, DELETE without a WHERE"],
  ["Cloud", "Deleting or terminating a bucket, a volume, an instance or a snapshot with a cloud command"],
  ["Containers", "docker rm or prune against a remote context"],
  ["Files", "rm -r"],
  ["Git", "A force push, or a branch delete, to a remote that isn’t local"],
];

/** What a listed name names, in words. */
const KIND: Record<GuardTargetKind, string> = {
  host: "Host name",
  profile: "Profile",
  project: "Project",
  context: "Context",
  workspace: "Workspace",
  bucket: "Storage bucket",
};

function Targets({ title, list, items }: { title: string; list: GuardList; items: GuardTarget[] }) {
  const source = `${list}_targets`;
  return (
    <div className="ap-targets" role="table" aria-label={title}>
      <div className="ap-target-row ap-head" role="row">
        <span role="columnheader">{title}</span>
        <span role="columnheader">Matched by</span>
        <span role="columnheader">Source</span>
      </div>
      {items.length === 0 ? <p className="ap-muted ap-note" role="note">Nothing is listed in {source} on this device.</p> : null}
      {items.map((item) => (
        <div key={item.id} className="ap-target-row" role="row">
          <span role="cell"><b>{item.pattern}</b><small>{item.note ?? KIND[item.kind]}</small></span>
          <span role="cell"><code>{item.kind} {item.pattern}</code></span>
          <span role="cell">{source}<small>This device’s list</small></span>
        </div>
      ))}
    </div>
  );
}

/** Settings › Crew › Production-linked targets: a hard rule, read-only, with this device's two lists. Adding to a list is not drawn here. */
export function ProductionTargets() {
  const read = useAutopilotGuards();
  const lists = useGuardTargets();
  if (read.status === "loading" || lists.status === "loading") return <Loading label="Reading the guard" rows={3} />;
  if (read.status === "error") return <ErrorState title="Can't read the guard" failure={read.error} onRetry={read.reload} />;
  if (lists.status === "error") return <ErrorState title="Can't read the guard's lists" failure={lists.error} onRetry={lists.reload} />;
  const secondKeys = read.data.second_key_engines;
  const items = lists.data.items;
  return (
    <section className="ap-set" aria-labelledby="ap-guard-title" data-frame="guard-settings">
      <header className="ap-set-head">
        <h2 id="ap-guard-title">Production-linked targets</h2>
        <Chip tone="plain">Preview</Chip>
      </header>
      <section className="ap-box">
        <h4>Always on, in every mode</h4>
        <p>A destructive command against a production-linked target needs two keys from models under Full: the lead’s and one from a different vendor. Under On and Off it is held for you. You can always approve a command yourself, once, as one deliberate action that names the exact command and the target; that makes you the final authority, not a second key, and it is logged. This is a hard rule, checked by Orbyn’s code before any model sees the request. Read-only: it can’t be switched off here, and nothing the lead decides can either.</p>
      </section>
      <section className="ap-box">
        <h4>What is production-linked · this device’s lists</h4>
        <p>Remote targets are guarded unless listed as safe. A remote database, a cloud account or project, a Kubernetes context, a Terraform workspace or a bucket is guarded, and an unknown remote is guarded. Local targets are not. Orbyn checks the command line: environment prefixes such as PGHOST and DATABASE_URL, the --profile, --context and --project flags, URLs and git remotes. It doesn’t read environment files or config files.</p>
      </section>
      <Targets title="Always guarded" list="prod" items={items.filter((item) => item.list === "prod")} />
      <Targets title="Exempt (safe)" list="safe" items={items.filter((item) => item.list === "safe")} />
      <p className="ap-muted">prod_targets are always guarded. safe_targets are exempt, for example a throwaway staging database. Both are this device’s lists, kept on this computer. They are not in .orbyn/crew.json, so a file in the repo can never loosen the guard.</p>
      <section className="ap-box">
        <h4>What counts as destructive</h4>
        <p>A list built into Orbyn, by category. A file in the repo can’t add to it, remove from it or loosen it; this device’s lists above are what change what counts as production-linked. Orbyn checks the command, not the worker’s description of it.</p>
        <div className="ap-targets" role="table" aria-label="What counts as destructive">
          <div className="ap-destroy-row ap-head" role="row">
            <span role="columnheader">Category</span>
            <span role="columnheader">Held for you, for example</span>
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
        <h4>Never approvable</h4>
        <p>kubectl delete, terraform destroy and helm uninstall are always denied, in every mode. They are on the always-denied list in Autopilot, not here.</p>
      </section>
      <section className="ap-box">
        <h4>The second key</h4>
        <p>
          It comes from an engine on a different vendor from the lead’s.{" "}
          {secondKeys.length ? `Signed in now: ${secondKeys.join(" and ")}. ` : ""}
          With none signed in, a guarded command is denied for now, and you can approve it yourself.
        </p>
      </section>
      <p className="ap-muted">{GUARD_WORKERS}</p>
    </section>
  );
}
