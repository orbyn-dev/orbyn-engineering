import { Check } from "lucide-react";
import { useId, useRef, useState } from "react";
import { toFailure } from "../../lib/useAsync";
import { Button, Field } from "../../ui/controls";
import { Dialog } from "../../ui/Dialog";
import { reposApi } from "./api";
import type { PublishedRepo, RepoRow } from "./contract";
import { ConfirmFacts, Path } from "./Dialogs";
import { isPublishName, isRemoteName, plain, PUBLISH_PROVIDERS } from "./model";

type Step = "provider" | "repository" | "done";
const STEP_LABEL: Record<Step, string> = { provider: "Step 1 of 3 · Provider", repository: "Step 2 of 3 · Repository", done: "Step 3 of 3 · Summary" };

/**
 * Publish, in three steps: where to host it, which repository to create, then the summary. Only the second step writes, and one
 * click on Publish is one write: the ref refuses a second click before the first renders and `busy` blocks Escape, Back and the
 * button while it runs. A failure stays on the second step and says why. The repository name is the person's text: it is checked
 * before it is sent and drawn only as text; the link to the host is built from the checked name, never taken from the service.
 */
export function PublishDialog({ projectId, repo, onClose, onDone }: { projectId: string; repo: RepoRow; onClose(): void; onDone(): void /* the list reads again; the dialog stays for the summary */ }) {
  const [step, setStep] = useState<Step>("provider");
  const [provider, setProvider] = useState<string | null>(null);
  const [name, setName] = useState(`orbyn-dev/${plain(repo.name)}`);
  const [visibility, setVisibility] = useState<"private" | "public">("private");
  const [remote, setRemote] = useState("origin");
  const [protocol, setProtocol] = useState<"ssh" | "https">("ssh");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<{ name: string; pushed: boolean } | null>(null);
  const running = useRef(false);
  const ids = { provider: useId(), visibility: useId(), protocol: useId() };
  const valid = isPublishName(name) && isRemoteName(remote);

  const publish = async () => {
    if (running.current || !valid) return;
    running.current = true;
    setBusy(true);
    setError(null);
    try {
      const sent = name.trim();
      const done: PublishedRepo = await reposApi().publish(projectId, repo.id, { name: sent, visibility, provider: "github", remote: remote.trim(), protocol });
      setResult({ name: sent, pushed: done.pushed !== false });
      setStep("done");
      onDone();
    } catch (failure) {
      setError(`Publish failed. ${toFailure(failure).message}`);
    } finally {
      running.current = false;
      setBusy(false);
    }
  };

  const footer = step === "provider" ? (
    <>
      <Button onClick={onClose}>Cancel</Button>
      <Button tone="primary" disabled={provider === null} onClick={() => setStep("repository")}>Next</Button>
    </>
  ) : step === "repository" ? (
    <>
      <Button disabled={busy} onClick={() => { setError(null); setStep("provider"); }}>Back</Button>
      <Button tone="primary" busy={busy} disabled={!valid} onClick={() => void publish()}>Publish</Button>
    </>
  ) : <Button tone="primary" data-dialog-focus onClick={onClose}>Done</Button>;

  return (
    <Dialog title="Publish repository" busy={busy} onClose={onClose} className="repos-dialog" footer={footer}>
      {step !== "done" ? <p className="field-hint">Pick where to host it, then point us at a repo to push to.</p> : null}
      <p className="repos-step">{STEP_LABEL[step]}</p>
      {step === "provider" ? (
        <fieldset className="repos-providers" aria-labelledby={ids.provider}>
          <legend id={ids.provider} className="sr-only">Provider</legend>
          {PUBLISH_PROVIDERS.map((entry) => (
            <label key={entry.id} className="repos-provider" data-ready={entry.ready || undefined}>
              <input type="radio" name={ids.provider} disabled={!entry.ready} checked={provider === entry.id} onChange={() => setProvider(entry.id)} />
              <span className="repos-provider-text"><b>{entry.name}</b><span className="field-hint">{entry.line}</span></span>
              {entry.ready ? null : <span className="repos-provider-setup">Setup Required</span>}
            </label>
          ))}
        </fieldset>
      ) : null}
      {step === "repository" ? (
        <>
          <Field label="Repository" error={name !== "" && !isPublishName(name) ? "Use owner/name." : undefined}>
            {(field) => <input {...field} data-dialog-focus className="dialog-field" value={name} spellCheck={false} onChange={(event) => setName(event.target.value)} />}
          </Field>
          <fieldset className="repos-visibility repos-visibility-cards" aria-labelledby={ids.visibility}>
            <legend id={ids.visibility}>Visibility</legend>
            {([["private", "Private", "Only invited people"], ["public", "Public", "Anyone on the web"]] as const).map(([value, label, line]) => (
              <label key={value}>
                <input type="radio" name={ids.visibility} checked={visibility === value} onChange={() => setVisibility(value)} />
                <span className="repos-provider-text"><b>{label}</b><span className="field-hint">{line}</span></span>
              </label>
            ))}
          </fieldset>
          <details className="repos-advanced">
            <summary>Advanced</summary>
            <Field label="Remote" error={remote !== "" && !isRemoteName(remote) ? "Use letters, digits, dot, dash or underscore." : undefined}>
              {(field) => <input {...field} className="dialog-field" value={remote} spellCheck={false} onChange={(event) => setRemote(event.target.value)} />}
            </Field>
            <fieldset className="repos-visibility" aria-labelledby={ids.protocol}>
              <legend id={ids.protocol}>Protocol</legend>
              {(["ssh", "https"] as const).map((value) => (
                <label key={value}><input type="radio" name={ids.protocol} checked={protocol === value} onChange={() => setProtocol(value)} />{value === "ssh" ? "SSH" : "HTTPS"}</label>
              ))}
            </fieldset>
          </details>
          <ConfirmFacts facts={[{ label: "Folder", value: <Path path={repo.path} />, mono: true }, { label: "Branch pushed", value: plain(repo.branch ?? "Unknown"), mono: true }]} />
          {error ? <p className="field-error" role="alert">{error}</p> : null}
        </>
      ) : null}
      {step === "done" && result ? (
        <div className="repos-published">
          <span className="repos-published-tick" aria-hidden><Check size={18} /></span>
          <b>{result.pushed ? "Repository published" : "Repository created"}</b>
          <p>{result.pushed ? `${plain(repo.branch ?? "The branch")} is now live on GitHub.` : `Remote “${plain(remote.trim())}” is set up. Make a commit and push it to share your code.`}</p>
          <ConfirmFacts facts={[{ label: "Repository", value: result.name, mono: true }]} />
          <a className="btn btn-secondary" href={`https://github.com/${result.name}`} target="_blank" rel="noopener noreferrer">Open on GitHub</a>
        </div>
      ) : null}
    </Dialog>
  );
}
