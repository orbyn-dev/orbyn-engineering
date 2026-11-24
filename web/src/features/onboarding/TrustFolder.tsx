import { useEffect, useState } from "react";
import { api } from "./api";
import { canSendPersonKey } from "../../api/personKey";
import { toFailure } from "../../lib/useAsync";
import { navigate, routeHref, type Route } from "../../product/state/route";
import { useWorkspace } from "../../product/state/workspace";
import { Button, LinkButton } from "../../ui/controls";
import { Empty, Loading } from "../../product/ui/states";
import { TRUST_CONFIRM, TRUST_DECLINE, TRUST_DESKTOP_ONLY, TRUST_TITLE, TrustBody } from "../../product/ui/TrustPrompt";
import "./onboarding.css";

export function TrustFolder({ projectId, threadId }: { projectId: string; threadId: string | null }) {
  const project = useWorkspace((state) => state.projects.data?.find((p) => p.id === projectId));
  const loaded = useWorkspace((state) => state.projects.data !== null);
  const putProject = useWorkspace((state) => state.putProject);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const next: Route = threadId
    ? { name: "thread", id: threadId }
    : { name: "new", projectId };
  const leave: Route = threadId ? { name: "thread", id: threadId } : { name: "home" };
  // Trusting is a `Person` route: only a page that holds the person key may ask. A browser tab says why instead of
  // offering a button the service would refuse.
  const canTrust = canSendPersonKey();

  useEffect(() => {
    if (project?.trusted) navigate(next);
    // `next` is derived from the props in this effect's identity.
  }, [project?.trusted, projectId, threadId]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!project) {
    return (
      <div className="page page-narrow">
        {loaded ? <Empty title="Project not found" /> : <Loading label="Loading project" />}
      </div>
    );
  }

  const trust = async () => {
    setBusy(true);
    setError(null);
    try {
      putProject(await api.trustProject(project.id));
    } catch (failure) {
      setError(`Couldn't trust this folder. ${toFailure(failure).message}`);
      setBusy(false);
    }
  };

  return (
    <div className="trust">
      <div className="trust-card" role="group" aria-labelledby="trust-title">
        <h1 id="trust-title" tabIndex={-1} data-page-title>
          {TRUST_TITLE(project.name)}
        </h1>
        <TrustBody path={project.path} />
        {canTrust ? null : <p>{TRUST_DESKTOP_ONLY}</p>}
        {error ? (
          <p className="field-error" role="alert">
            {error}
          </p>
        ) : null}
        <div className="trust-actions">
          <LinkButton href={routeHref(leave)}>{TRUST_DECLINE}</LinkButton>
          {canTrust ? (
            <Button tone="primary" busy={busy} onClick={() => void trust()}>
              {TRUST_CONFIRM}
            </Button>
          ) : null}
        </div>
      </div>
    </div>
  );
}
