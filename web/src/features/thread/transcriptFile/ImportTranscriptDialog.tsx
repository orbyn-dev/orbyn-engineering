import { CircleAlert, FileText, Loader2 } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { ApiFailure } from "../../../api/client";
import type { Project } from "../../../api/types";
import type { TranscriptImportAnswer } from "../../../generated/TranscriptImportAnswer";
import { toFailure } from "../../../lib/useAsync";
import { navigate } from "../../../product/state/route";
import { useWorkspace } from "../../../product/state/workspace";
import { Button } from "../../../ui/controls";
import { Dialog } from "../../../ui/Dialog";
import { MAX_IMPORT_BYTES, importTranscript, readProjectPreferences } from "./api";
import { EngineSelect } from "./EngineSelect";
import "./transcriptFile.css";

export interface ImportTranscriptDialogProps {
  project: Project;
  file: File;
  onClose(): void;
  onChooseAnother(): void;
}

interface Refusal {
  message: string;
  detail: string | null;
  chooseAnother: boolean;
}

const failureOf = (error: unknown): ApiFailure =>
  error instanceof ApiFailure ? error : new ApiFailure("unknown", toFailure(error).message, 0);

/** The service's refusal sentences, replacing the preview; anything else shows the service's own words. */
function refusalFor(failure: ApiFailure): Refusal {
  if (failure.code === "file_too_large") {
    return { message: "A transcript is at most 2 MiB.", detail: null, chooseAnother: true };
  }
  if (failure.code === "transcript_too_new") {
    return { message: "This transcript was made by a newer Orbyn.", detail: "Update Orbyn, then try again.", chooseAnother: true };
  }
  if (failure.code === "transcript_unreadable") {
    return { message: "This file is not an Orbyn transcript.", detail: "It may be damaged, or made by another app.", chooseAnother: true };
  }
  if (failure.code === "desktop_required" || failure.code === "person_required") {
    return { message: "This action is only available in the desktop app.", detail: null, chooseAnother: false };
  }
  if (failure.status === 404) {
    return { message: "That engine is not available here. Choose another.", detail: null, chooseAnother: true };
  }
  return { message: failure.message, detail: null, chooseAnother: true };
}

/** The file row's size, as the sheet draws it: 184 KB, 3.4 MiB. */
function formatFileSize(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${Number((bytes / (1024 * 1024)).toFixed(1))} MiB`;
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${bytes} B`;
}

const preference = (rows: { key: string; value: unknown }[] | null, key: string): unknown =>
  rows?.find((row) => row.key === key)?.value;

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="transcript-row">
      <span className="transcript-label">{label}</span>
      <span className="transcript-value">{children}</span>
    </div>
  );
}

/**
 * Imports one transcript file into one project: a dry run first, then the title, message count, dropped problems and the
 * engine this thread starts on, then the import itself. The sheet only shows what the service or the file's own `thread.engine`
 * label says; it never shows a message. A refusal replaces the preview and keeps the sheet open.
 */
export function ImportTranscriptDialog({ project, file, onClose, onChooseAnother }: ImportTranscriptDialogProps) {
  const engines = useWorkspace((state) => state.engines);
  const [preferences, setPreferences] = useState<{ key: string; value: unknown }[] | null>(null);
  const [answer, setAnswer] = useState<TranscriptImportAnswer | null>(null);
  const [failure, setFailure] = useState<ApiFailure | null>(null);
  const [pickedEngine, setPickedEngine] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);
  const [fileEngine, setFileEngine] = useState<{ read: boolean; value: string | null }>({ read: false, value: null });
  const live = useRef(true);

  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);

  useEffect(() => {
    const controller = new AbortController();
    readProjectPreferences(project.id, controller.signal).then(
      (rows) => {
        if (!controller.signal.aborted) setPreferences(rows as unknown as { key: string; value: unknown }[]);
      },
      () => {
        if (!controller.signal.aborted) setPreferences([]);
      },
    );
    return () => controller.abort();
  }, [project.id]);

  useEffect(() => {
    let current = true;
    void (async () => {
      try {
        // A file over the cap is refused by the dry run; never read all of it into memory here.
        if (file.size > MAX_IMPORT_BYTES) throw new Error("too large");
        const text = await file.text();
        const document = JSON.parse(text) as { thread?: { engine?: unknown } };
        if (!current) return;
        const engine = document.thread?.engine;
        setFileEngine({ read: true, value: typeof engine === "string" && engine.trim() !== "" ? engine : null });
      } catch {
        if (current) setFileEngine({ read: false, value: null });
      }
    })();
    return () => {
      current = false;
    };
  }, [file]);

  const disabledEngines = useMemo(() => {
    const value = preference(preferences, "disabled_engines");
    return Array.isArray(value) ? value.filter((entry): entry is string => typeof entry === "string") : [];
  }, [preferences]);
  const allowed = useMemo(
    () => (engines.data ?? []).filter((engine) => engine.installed && !disabledEngines.includes(engine.id)),
    [engines.data, disabledEngines],
  );
  const defaultEngine = useMemo(() => {
    const value = preference(preferences, "default_engine");
    return typeof value === "string" && value !== "" && allowed.some((engine) => engine.id === value) ? value : null;
  }, [preferences, allowed]);
  const enginesReady = engines.data !== null || engines.error !== null;

  useEffect(() => {
    if (preferences === null || !enginesReady) return;
    const list = engines.data ?? [];
    const disabled = preference(preferences, "disabled_engines");
    const off = Array.isArray(disabled) ? disabled.filter((entry): entry is string => typeof entry === "string") : [];
    const installed = list.filter((engine) => engine.installed && !off.includes(engine.id));
    const wanted = preference(preferences, "default_engine");
    const named = typeof wanted === "string" && wanted !== "" && installed.some((engine) => engine.id === wanted) ? wanted : null;
    const validation = named ?? installed[0]?.id ?? null;
    const controller = new AbortController();
    importTranscript(
      { projectId: project.id, ...(validation ? { engine: validation } : {}), dryRun: true, file },
      controller.signal,
    ).then(
      (result) => {
        if (!controller.signal.aborted && live.current) setAnswer(result);
      },
      (error: unknown) => {
        if (controller.signal.aborted || !live.current) return;
        if (error instanceof DOMException && error.name === "AbortError") return;
        setFailure(failureOf(error));
      },
    );
    return () => controller.abort();
  }, [preferences, enginesReady, engines.data, project.id, file]);

  const chosen = pickedEngine ?? defaultEngine ?? "";
  const refusal = failure ? refusalFor(failure) : null;
  const fileEngineLabel = fileEngine.value
    ? (engines.data?.find((engine) => engine.id === fileEngine.value)?.name ?? fileEngine.value)
    : null;
  const note =
    defaultEngine === null
      ? fileEngine.read
        ? fileEngineLabel
          ? `The file was made with ${fileEngineLabel}.`
          : "The file names no engine."
        : null
      : !fileEngine.read
        ? "Project default."
        : !fileEngineLabel
          ? "Project default. The file names no engine."
          : fileEngine.value !== defaultEngine
            ? `Project default. The file was made with ${fileEngineLabel}.`
            : "Project default.";
  const shownProblems = answer?.problems.filter((problem) => problem.kind === "message") ?? [];
  const canImport = answer !== null && refusal === null && chosen !== "" && !importing;

  const importNow = async () => {
    if (!answer || importing || chosen === "") return;
    setImporting(true);
    try {
      const done = await importTranscript({ projectId: project.id, engine: chosen, dryRun: false, file });
      if (!done.thread) throw new ApiFailure("import_failed", "The service did not answer a thread.", 500);
      useWorkspace.getState().putThread(done.thread);
      onClose();
      navigate({ name: "thread", id: done.thread.id });
    } catch (error) {
      setFailure(failureOf(error));
      setImporting(false);
    }
  };

  const footer = refusal ? (
    <>
      {refusal.chooseAnother ? (
        <Button tone="quiet" onClick={onChooseAnother}>
          Choose another file…
        </Button>
      ) : null}
      <Button tone="primary" onClick={onClose}>
        Close
      </Button>
    </>
  ) : (
    <>
      <Button tone="quiet" onClick={onClose} disabled={importing} data-dialog-focus={!importing || undefined}>
        Cancel
      </Button>
      <Button tone="primary" disabled={!canImport} busy={importing} onClick={() => void importNow()}>
        {importing ? "Importing…" : "Import as new thread"}
      </Button>
    </>
  );

  return (
    <Dialog title="Import transcript" onClose={onClose} busy={importing} footer={footer}>
      <p className="transcript-import-sub">Into {project.name}, as a new thread.</p>
      <div className="transcript-file">
        <FileText size={16} aria-hidden />
        <span className="transcript-file-name">{file.name}</span>
        <span className="transcript-file-size">{formatFileSize(file.size)}</span>
      </div>
      {refusal ? (
        <div className="transcript-refusal" role="alert">
          <CircleAlert size={16} aria-hidden />
          <div>
            <p>{refusal.message}</p>
            {refusal.detail ? <p className="transcript-refusal-detail">{refusal.detail}</p> : null}
          </div>
        </div>
      ) : answer === null ? (
        <p className="transcript-reading" role="status">
          <Loader2 size={13} className="spin" aria-hidden />
          Reading file…
        </p>
      ) : (
        <>
          <div className="transcript-preview">
            <Row label="Title">
              <span>{answer.title}</span>
            </Row>
            <Row label="Messages">{answer.message_count}</Row>
          </div>
          {answer.dropped > 0 && shownProblems.length > 0 ? (
            <div className="transcript-problems">
              <div className="transcript-problems-head">
                <span className="transcript-dropped">{answer.dropped} dropped</span>
                <span className="transcript-problems-note">Their text is not shown. The rest imports.</span>
              </div>
              <ul className="transcript-problem-list" aria-label="Dropped messages">
                {shownProblems.map((problem, at) =>
                  problem.index === null ? null : (
                    <li key={`${problem.index}-${problem.code}-${at}`}>
                      <span>Message {problem.index + 1}</span>
                      <code>{problem.code}</code>
                    </li>
                  ),
                )}
              </ul>
            </div>
          ) : null}
          <EngineSelect
            engines={allowed}
            value={chosen}
            defaultEngine={defaultEngine}
            note={note}
            disabled={importing}
            onChange={setPickedEngine}
          />
        </>
      )}
    </Dialog>
  );
}

