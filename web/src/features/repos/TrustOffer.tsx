import { useState } from "react";
import { useWorkspace } from "../../product/state/workspace";
import { Button } from "../../ui/controls";
import { useThreadRepos } from "./api";
import { plain } from "./model";
import { TrustRepo } from "./TrustRepo";

/** The service's own words about a refusal are text from outside: one line, capped. */
const MESSAGE_CAP = 200;
const capped = (text: string) => { const line = plain(text); return line.length > MESSAGE_CAP ? `${line.slice(0, MESSAGE_CAP - 1)}…` : line; };

/**
 * What a file of a repository that is not trusted (`409 repo_untrusted`) shows in place of a refused read or write: the service's
 * message as text and one button that opens the trust dialog the repository's Git menu opens. The refused call is not repeated
 * by trusting; `onDone` is where the caller lets the person try again.
 */
export function TrustOffer({ threadId, repoId, message, onDone }: { threadId: string; repoId: string; message: string; onDone?(): void }) {
  const projectId = useWorkspace((state) => state.threads[threadId]?.project_id);
  const { repos } = useThreadRepos(threadId);
  const [trusting, setTrusting] = useState(false);
  const name = repos?.find((repo) => repo.repo_id === repoId)?.name ?? "This repository";
  return (
    <>
      <span>{capped(message)}</span>
      {projectId ? <Button small onClick={() => setTrusting(true)}>Trust repository…</Button> : null}
      {trusting && projectId ? <TrustRepo projectId={projectId} repoId={repoId} name={name} onClose={() => setTrusting(false)} onDone={() => { setTrusting(false); onDone?.(); }} /> : null}
    </>
  );
}
