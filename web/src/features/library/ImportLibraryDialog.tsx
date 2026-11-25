import { CircleAlert } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { ApiFailure, NETWORK_ERROR } from "../../api/client";
import { toFailure } from "../../lib/useAsync";
import { Button } from "../../ui/controls";
import { Dialog } from "../../ui/Dialog";
import { libraryApi } from "./api";
import type { ImportAnswer } from "./contract";
import { importNotice, plain } from "./model";

/** How many refused entries, renamed names and dropped parts the sheet lists; the totals always count them all. */
const SHOWN = 12;

const IMPORT_KIND = new Map<string, string>([["profile", "Profiles"], ["workflow", "Workflows"], ["prompt", "Prompts"], ["env_set", "Env sets"]]);
const KIND_ORDER = ["profile", "workflow", "prompt", "env_set"];
const kindName = (kind: string): string => IMPORT_KIND.get(kind) ?? plain(kind, 40);

/** The two codes the service puts in `problems`, in words; any other stable word is shown as it came. */
const DROPPED = new Map<string, string>([
  ["unknown_env_set", "a set this computer does not have, so that link is left out."],
  ["value_dropped", "an environment value; only key names are imported."],
]);
const droppedText = (code: string): string => DROPPED.get(code) ?? "the service left this part out.";

/** A file size as the sheet draws it: 184 KB, 2.4 MiB. */
const sizeText = (bytes: number): string =>
  bytes >= 1024 * 1024 ? `${Number((bytes / (1024 * 1024)).toFixed(1))} MiB` : bytes >= 1024 ? `${Math.round(bytes / 1024)} KB` : `${bytes} B`;

/** A refusal the sheet replaces the preview with: what happened, and whether another file can fix it. */
interface Refusal { message: string; detail: string | null; chooseAnother: boolean }

/**
 * The report as this sheet draws it: the dry-run flag it asked for, three counts and two lists. A foreign body is refused as a
 * shape this app does not know, never drawn as counts, so a service that answers something else cannot crash the sheet.
 */
function readAnswer(value: unknown, dryRun: boolean): ImportAnswer {
  const answer = typeof value === "object" && value !== null && !Array.isArray(value) ? (value as Partial<ImportAnswer>) : null;
  const known = answer !== null
    && answer.dry_run === dryRun
    && Array.isArray(answer.lines)
    && Array.isArray(answer.problems)
    && [answer.created, answer.renamed, answer.refused].every((count) => typeof count === "number" && Number.isInteger(count) && count >= 0);
  if (!known || answer === null) throw new ApiFailure("bad_answer", "The service answered an import in a shape this app does not know.", 502);
  return answer as ImportAnswer;
}

/**
 * The words for a failed read or import. The local codes are the file the page read itself; every other line is the service's
 * own sentence, kept as it came (`plain` cuts it so a hostile body cannot grow the sheet). A `404` is a service too old for the
 * route, not a bad file, so it says so and does not offer another file.
 */
function refusalFor(error: unknown, limit: number | undefined): Refusal {
  const failure = error instanceof ApiFailure ? error : toFailure(error);
  if (failure.code === "file_too_large") return { message: "This file is too large to import.", detail: typeof limit === "number" ? `An import file is at most ${sizeText(limit)}.` : null, chooseAnother: true };
  if (failure.code === "bad_answer") return { message: "The service answered an import in a shape this app does not know.", detail: "Update Orbyn, then try again.", chooseAnother: false };
  if (failure.code === "not_json") return { message: "This file is not JSON.", detail: "Import reads an Orbyn library export file.", chooseAnother: true };
  if (failure.code === "unreadable") return { message: "This file could not be read.", detail: "Choose it again, or choose another file.", chooseAnother: true };
  if (failure.code === "desktop_required" || failure.code === "person_required") return { message: "Import is only available in the Orbyn desktop app.", detail: null, chooseAnother: false };
  if (failure.status === 404 || failure.code === "not_found") return { message: "This version of the Orbyn service has no library import.", detail: "Update Orbyn, then try again.", chooseAnother: false };
  if (failure.code === NETWORK_ERROR) return { message: "Can’t reach the Orbyn service.", detail: "Check that Orbyn is running, then try again.", chooseAnother: true };
  if (failure.status === 400) return { message: "The service refused this document.", detail: plain(failure.message, 240) || null, chooseAnother: true };
  if (failure.status === 409) return { message: "The service refused the import.", detail: plain(failure.message, 240) || null, chooseAnother: true };
  if (failure.status === 422) return { message: "The service couldn’t read this document.", detail: plain(failure.message, 240) || null, chooseAnother: true };
  return { message: "Couldn’t import this file.", detail: plain(failure.message, 240) || null, chooseAnother: true };
}

/** One entry the service refused, or one part it dropped from an entry it admitted. */
function Note({ what, code, text }: { what: string; code: string; text?: string }) {
  return (
    <li>
      <span>{what}</span>
      <code>{plain(code, 40)}</code>
      {text ? <span className="lib-note">{text}</span> : null}
    </li>
  );
}

/** Counts per kind, the names a clash renamed, and the refused entries and dropped parts, all from the service's answer. */
function Report({ answer }: { answer: ImportAnswer }) {
  const counts = new Map<string, { created: number; renamed: number; refused: number }>();
  for (const line of answer.lines) {
    const held = counts.get(line.kind) ?? { created: 0, renamed: 0, refused: 0 };
    held[line.outcome] += 1;
    counts.set(line.kind, held);
  }
  const kinds = [...KIND_ORDER.filter((kind) => counts.has(kind)), ...[...counts.keys()].filter((kind) => !KIND_ORDER.includes(kind))];
  const renamed = answer.lines.filter((line) => line.outcome === "renamed" && typeof line.name === "string" && line.name !== "");
  const refused = answer.lines.filter((line) => line.outcome === "refused");
  const problems = answer.problems;
  return (
    <>
      {kinds.length === 0 ? (
        <p className="lib-note">This file holds no entries to import.</p>
      ) : (
        <table className="lib-table" aria-label="Import by kind">
          <thead><tr><th>Kind</th><th>Created</th><th>Renamed</th><th>Refused</th></tr></thead>
          <tbody>
            {kinds.map((kind) => {
              const held = counts.get(kind) ?? { created: 0, renamed: 0, refused: 0 };
              return <tr key={kind}><th scope="row">{kindName(kind)}</th><td>{held.created}</td><td>{held.renamed}</td><td>{held.refused}</td></tr>;
            })}
          </tbody>
          <tfoot><tr><th scope="row">All</th><td>{answer.created}</td><td>{answer.renamed}</td><td>{answer.refused}</td></tr></tfoot>
        </table>
      )}
      {renamed.length > 0 ? (
        <p className="lib-note">Saved under a free name: {renamed.slice(0, SHOWN).map((line) => plain(line.name, 60)).join(", ")}{renamed.length > SHOWN ? `, and ${renamed.length - SHOWN} more` : ""}.</p>
      ) : null}
      {refused.length > 0 ? (
        <div className="lib-import-group">
          <h3 className="lib-group-title">Refused · {refused.length}</h3>
          <p className="lib-note">The service refused these entries. Nothing was written for them.</p>
          <ul className="lib-import-list" aria-label="Refused entries">
            {refused.slice(0, SHOWN).map((line) => <Note key={`${line.kind}:${line.index}`} what={`${kindName(line.kind)} #${line.index}`} code={line.code ?? "refused"} />)}
            {refused.length > SHOWN ? <li className="lib-note">And {refused.length - SHOWN} more refused.</li> : null}
          </ul>
        </div>
      ) : null}
      {problems.length > 0 ? (
        <div className="lib-import-group">
          <h3 className="lib-group-title">Dropped · {problems.length}</h3>
          <ul className="lib-import-list" aria-label="Dropped parts">
            {problems.slice(0, SHOWN).map((problem) => <Note key={`${problem.kind}:${problem.index}:${problem.code}`} what={`${kindName(problem.kind)} #${problem.index}`} code={problem.code} text={droppedText(problem.code)} />)}
            {problems.length > SHOWN ? <li className="lib-note">And {problems.length - SHOWN} more dropped.</li> : null}
          </ul>
        </div>
      ) : null}
    </>
  );
}

type Phase =
  | { kind: "checking" }
  | { kind: "refused"; refusal: Refusal }
  | { kind: "preview"; document: unknown; answer: ImportAnswer }
  | { kind: "applied"; answer: ImportAnswer };

/**
 * The library import sheet: one file the person picked, a dry run by the service, then the report and, only on Import, the
 * real call. Nothing is written before that click. The dry run answers counts per kind, the names a clash renames, the entries
 * the service refuses and the parts it drops; Apply repeats the same request with `dry_run: false` and the caller reloads the
 * library. Every word from the service or the file is `plain` text, never markup. There is no design frame for this sheet; it
 * uses the library's own `Dialog` and its report reuses the list and table styles.
 */
export function ImportLibraryDialog({ file, limit, onChooseAnother, onApplied, onClose }: {
  file: File;
  /** The service's own import cap (`summary.limits.import_bytes`): a larger file is refused before it is read. Absent, the service still refuses it. */
  limit?: number;
  onChooseAnother(): void;
  onApplied(answer: ImportAnswer): void;
  onClose(): void;
}) {
  const [phase, setPhase] = useState<Phase>({ kind: "checking" });
  const [applying, setApplying] = useState(false);
  const [applyFailure, setApplyFailure] = useState<string | null>(null);
  const pending = useRef(false);

  useEffect(() => {
    let live = true;
    setPhase({ kind: "checking" });
    setApplyFailure(null);
    void (async () => {
      try {
        if (typeof limit === "number" && Number.isFinite(limit) && file.size > limit) throw new ApiFailure("file_too_large", "", 413);
        let text: string;
        try {
          text = await file.text();
        } catch {
          throw new ApiFailure("unreadable", "", 0);
        }
        let document: unknown;
        try {
          document = JSON.parse(text) as unknown;
        } catch {
          throw new ApiFailure("not_json", "", 400);
        }
        if (!live) return;
        const answer = readAnswer(await libraryApi().importLibrary(document, true), true);
        if (live) setPhase({ kind: "preview", document, answer });
      } catch (error) {
        if (live) setPhase({ kind: "refused", refusal: refusalFor(error, limit) });
      }
    })();
    return () => {
      live = false;
    };
  }, [file, limit]);

  const apply = async () => {
    if (phase.kind !== "preview" || pending.current) return;
    pending.current = true;
    setApplying(true);
    setApplyFailure(null);
    try {
      const answer = readAnswer(await libraryApi().importLibrary(phase.document, false), false);
      setPhase({ kind: "applied", answer });
      onApplied(answer);
    } catch (error) {
      const refused = refusalFor(error, limit);
      setApplyFailure([refused.message, refused.detail].filter((part): part is string => typeof part === "string" && part !== "").join(" "));
    } finally {
      pending.current = false;
      setApplying(false);
    }
  };

  const answer = phase.kind === "preview" || phase.kind === "applied" ? phase.answer : null;
  const added = answer === null ? 0 : answer.created + answer.renamed;
  const refused = phase.kind === "refused" ? phase.refusal : null;
  const applied = phase.kind === "applied";
  const importLabel = answer === null ? "Import" : added === 0 ? "Nothing to import" : `Import ${added} ${added === 1 ? "item" : "items"}`;

  const footer = refused ? (
    <>
      {refused.chooseAnother ? <Button tone="quiet" onClick={onChooseAnother}>Choose another file…</Button> : null}
      <Button tone="primary" onClick={onClose} data-dialog-focus>Close</Button>
    </>
  ) : applied ? (
    <Button tone="primary" onClick={onClose} data-dialog-focus>Close</Button>
  ) : (
    <>
      <Button onClick={onClose} disabled={applying} data-dialog-focus>Cancel</Button>
      <Button tone="primary" disabled={answer === null || added === 0 || applying} busy={applying} onClick={() => void apply()}>{importLabel}</Button>
    </>
  );

  return (
    <Dialog title="Import library" onClose={onClose} busy={applying} className="lib-dialog-wide" footer={footer}>
      <p className="lib-note">From <b>{plain(file.name, 80)}</b> · {sizeText(file.size)}. Into Personal, on this computer; nothing is written into a repository.</p>
      {refused ? (
        <div className="lib-refusal" role="alert">
          <CircleAlert size={16} aria-hidden />
          <div>
            <p>{refused.message}</p>
            {refused.detail ? <p className="lib-refusal-detail">{refused.detail}</p> : null}
          </div>
        </div>
      ) : phase.kind === "checking" ? (
        <p className="lib-note" role="status">Reading the file…</p>
      ) : answer ? (
        <>
          {applied ? <p className="lib-done" role="status">{importNotice(answer)}</p> : <p className="lib-note" role="status">Checked by the service. Nothing is written until you choose Import.</p>}
          <Report answer={answer} />
          {applyFailure ? <p className="field-error" role="alert">{applyFailure}</p> : null}
        </>
      ) : null}
    </Dialog>
  );
}
