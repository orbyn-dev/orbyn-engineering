import { Check, CircleDashed, ExternalLink, Pencil, Plus, X } from "lucide-react";
import { useState } from "react";
import type { PullCheck, PullRequestDetail } from "../../../api/types";
import { topics } from "../../../lib/dataRefresh";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { useUi } from "../../../product/state/ui";
import { Button, IconButton, Pill } from "../../../ui/controls";
import { Menu, type MenuItem } from "../../../ui/Menu";
import { Presence } from "../../../ui/Presence";
import { Markdown } from "../../content";
import { pullsApi } from "../api";

const checkIcon = (check: PullCheck) =>
  check.state === "pass" ? (
    <Check size={14} className="tone-success" role="img" aria-label="Passing" />
  ) : check.state === "fail" ? (
    <X size={14} className="tone-danger" role="img" aria-label="Failing" />
  ) : (
    <CircleDashed size={14} className="tone-warn" role="img" aria-label="Pending" />
  );

/** A fork's workflow waits for approval on a run named only by its link. */
const waitingRun = (check: PullCheck): number | null => {
  if (check.state !== "pending" || !/approv/i.test(check.description ?? "")) return null;
  const run = /\/actions\/runs\/(\d+)/.exec(check.url ?? "");
  return run ? Number(run[1]) : null;
};

export function Checks({ projectId, number }: { projectId: string; number: number }) {
  const checks = useAsync(`pull-checks:${projectId}:${number}`, (signal) => pullsApi.checks(projectId, number, signal), { keep: true, refreshOn: topics(`pull:${projectId}:${number}`) });
  const pushToast = useUi((state) => state.pushToast);
  if (checks.status !== "ready" || checks.data.length === 0) return null;
  const approve = (run: number) => {
    pullsApi.approveWorkflow(projectId, run).then(
      () => {
        pushToast("Workflow approved");
        checks.reload();
      },
      (failure: unknown) => pushToast(`Couldn't approve it. ${toFailure(failure).message}`),
    );
  };
  return (
    <section className="pull-section pull-card">
      <h3>Checks</h3>
      <ul className="pull-checks">
        {checks.data.map((check) => {
          const run = waitingRun(check);
          return (
            <li key={`${check.workflow ?? ""}:${check.name}`}>
              {checkIcon(check)}
              <span>{check.name}</span>
              {check.description ? <small className="muted">{check.description}</small> : null}
              {check.url ? (
                <a href={check.url} target="_blank" rel="noreferrer" aria-label={`Open ${check.name}`}>
                  <ExternalLink size={12} aria-hidden />
                </a>
              ) : null}
              {run !== null ? (
                <Button small onClick={() => approve(run)}>
                  Approve workflow
                </Button>
              ) : null}
            </li>
          );
        })}
      </ul>
    </section>
  );
}

/** Reviewers or labels: what's set, and a search over what the forge offers. */
export function People({
  title,
  chosen,
  load,
  save,
  refreshTopic,
}: {
  title: string;
  /** The service topic that says the candidates changed. */
  refreshTopic?: string;
  chosen: string[];
  load(signal: AbortSignal): Promise<{ name: string }[]>;
  save(add: string[], remove: string[]): Promise<void>;
}) {
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const candidates = useAsync(`pull-${title}`, load, { refreshOn: refreshTopic ? topics(refreshTopic) : undefined });
  const pushToast = useUi((state) => state.pushToast);
  const change = (add: string[], remove: string[]) => {
    save(add, remove).catch((failure: unknown) => pushToast(`Couldn't change that. ${toFailure(failure).message}`));
  };
  const items: MenuItem[] = (candidates.data ?? []).map((candidate) => {
    const on = chosen.includes(candidate.name);
    return {
      label: candidate.name,
      selected: on,
      onSelect: () => (on ? change([], [candidate.name]) : change([candidate.name], [])),
    };
  });
  return (
    <div className="pull-people pull-card">
      <span className="pull-card-title">{title}</span>
      {chosen.map((name) => (
        <Pill key={name}>{name}</Pill>
      ))}
      <IconButton
        label={`Change ${title.toLowerCase()}`}
        onClick={(event) => {
          const rect = event.currentTarget.getBoundingClientRect();
          setMenu({ x: rect.left, y: rect.bottom + 4 });
        }}
      >
        <Plus size={14} aria-hidden />
      </IconButton>
      <Presence>{menu ? <Menu x={menu.x} y={menu.y} label={title} items={items} onClose={() => setMenu(null)} /> : null}</Presence>
    </div>
  );
}

export function Description({
  detail,
  projectId,
  onSave,
}: {
  detail: PullRequestDetail;
  projectId: string;
  onSave(body: string): Promise<void>;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  if (draft !== null) {
    return (
      <div className="pull-section">
        <textarea aria-label="Description" rows={6} value={draft} onChange={(event) => setDraft(event.target.value)} />
        <div className="pull-form-actions">
          <Button small onClick={() => setDraft(null)}>
            Cancel
          </Button>
          <Button small tone="primary" onClick={() => void onSave(draft).then(() => setDraft(null))}>
            Save description
          </Button>
        </div>
      </div>
    );
  }
  return (
    <>
      {detail.body ? <Markdown text={detail.body} projectId={projectId} /> : <p className="muted">No description provided.</p>}
      <IconButton label="Edit description" onClick={() => setDraft(detail.body)}>
        <Pencil size={14} aria-hidden />
      </IconButton>
    </>
  );
}
