import { Download, Plus, Search } from "lucide-react";
import { useRef, useState } from "react";
import { ApiFailure } from "../../api/client";
import { saveBlob } from "../../lib/download";
import { useUi } from "../../product/state/ui";
import { useWorkspace } from "../../product/state/workspace";
import { Button } from "../../ui/controls";
import { libraryApi, useLibrary, useSuggestions, type LibraryData } from "./api";
import type { EnvSet, Kind, Prompt, RepoEnvSet, RepoPrompt, RepoWorkflow, Suggestions, SuggestionFile, Workflow } from "./contract";
import { ItemForm } from "./ItemForm";
import { exportFileName, plain, sourceLabel } from "./model";
import { DeleteItem, EnvSetPage, PromptPage, WorkflowPage } from "./Pages";
import { UseDialog, type UseMode } from "./UseDialog";
import "./library.css";

type Tab = "repo" | "personal";
type Pick = { origin: Tab; kind: Kind; key: string };
type Open =
  | { what: "form"; kind: Kind | null; item: Workflow | Prompt | EnvSet | null }
  | { what: "use"; mode: UseMode; kind: "workflow" | "prompt"; item: Workflow | Prompt }
  | { what: "delete"; kind: Kind; id: string; name: string };

/** One repository entry is named by its own file and its index in that file. */
const refKey = (entry: { file: SuggestionFile; index: number }) => `${entry.file}#${entry.index}`;
const KIND_TITLE: Record<Kind, string> = { workflow: "Workflows", prompt: "Prompts", env_set: "Env sets" };

const matches = (query: string, ...parts: unknown[]) => query === "" || parts.some((part) => typeof part === "string" && part.toLowerCase().includes(query));
const count = (n: number, one: string, many = `${one}s`) => (n === 0 ? `no ${many}` : `${n} ${n === 1 ? one : many}`);

/** Types text into a thread's main terminal with no Enter and says whether it did. The panels feature owns the terminal, so it passes this in. */
export type TypeIntoTerminal = (threadId: string, text: string) => boolean;

/** What the three insert modes do with the text the service rendered. The page never runs it. */
async function place(threadId: string, mode: Exclude<UseMode, "run">, text: string, typeIntoTerminal: TypeIntoTerminal): Promise<string | null> {
  if (mode === "composer") {
    const held = useUi.getState().drafts[threadId] ?? "";
    useUi.getState().setDraft(threadId, held.trim() === "" ? text : `${held}\n${text}`);
    return "Added to your message. You can still edit it; nothing is sent.";
  }
  if (mode === "terminal") {
    return typeIntoTerminal(threadId, text) ? "Typed into the terminal line. Press Enter there to run it." : "No terminal is open for this thread. Open the terminal, then insert it again.";
  }
  try {
    await navigator.clipboard.writeText(text);
    return "Copied.";
  } catch {
    return "Couldn’t copy. Select the text above and copy it.";
  }
}

/**
 * The Library panel (frames `workflow-*`; docs/execution-profiles-and-library.md, Part 2): saved workflows, prompts and environment sets,
 * in two tabs. From repo lists the entries the project commits in `.orbyn/`, read only; Personal holds the person's own items, on this
 * computer. Hidden entirely when the service has no library route. Nothing is written into a repository; a repository entry is
 * used by copying it to Personal first. Everything the service or a file says is text, escaped and cut. The toolbar's Export is one
 * quiet button, with no design frame and no screen of its own: it saves the service's document and writes nothing. Planned, not drawn:
 * the last run of a workflow, "changed since the last commit", another worktree's items, Compare, Reveal here, applying a set to a
 * shell, and number or yes/no parameters (the contract has text, choice and path).
 */
export function LibraryPanel({ threadId, typeIntoTerminal }: { threadId: string; typeIntoTerminal: TypeIntoTerminal }) {
  const { data, error, settled, reload } = useLibrary();
  const projectId = useWorkspace((state) => state.threads[threadId]?.project_id) ?? "";
  const [tab, setTab] = useState<Tab>("repo");
  const [query, setQuery] = useState("");
  const [pick, setPick] = useState<Pick | null>(null);
  const [open, setOpen] = useState<Open | null>(null);
  const [notice, setNotice] = useState("");
  if (data === null && !error) return settled ? null : <section className="lib" aria-busy="true" aria-label="Library" />;
  if (data === null) return <section className="lib"><p className="field-error" role="alert">{plain(error?.message, 200)}</p></section>;
  return (
    <LibraryBody
      threadId={threadId} projectId={projectId} data={data} reload={reload} typeIntoTerminal={typeIntoTerminal}
      tab={tab} setTab={(next) => { setTab(next); setPick(null); }} query={query} setQuery={setQuery}
      pick={pick} setPick={setPick} open={open} setOpen={setOpen} notice={notice} setNotice={setNotice}
    />
  );
}

function LibraryBody({ threadId, projectId, data, reload, typeIntoTerminal, tab, setTab, query, setQuery, pick, setPick, open, setOpen, notice, setNotice }: {
  threadId: string; projectId: string; data: LibraryData; reload(): void; typeIntoTerminal: TypeIntoTerminal;
  tab: Tab; setTab(tab: Tab): void; query: string; setQuery(query: string): void;
  pick: Pick | null; setPick(pick: Pick | null): void; open: Open | null; setOpen(open: Open | null): void; notice: string; setNotice(text: string): void;
}) {
  const repoRead = useSuggestions(projectId);
  const suggestions: Suggestions = repoRead.data ?? { workflows: [], prompts: [], env_sets: [], problems: [] };
  const copying = useRef(false);
  const [busyCopy, setBusyCopy] = useState(false);
  const needle = query.trim().toLowerCase();

  const repoWorkflows = suggestions.workflows.filter((one) => matches(needle, one.name, one.description, one.command));
  const repoPrompts = suggestions.prompts.filter((one) => matches(needle, one.name, one.description, one.body));
  const repoSets = suggestions.env_sets.filter((one) => matches(needle, one.name, one.description, ...one.keys));
  const workflows = data.workflows.filter((one) => matches(needle, one.name, one.description, one.command));
  const prompts = data.prompts.filter((one) => matches(needle, one.name, one.description, one.body));
  const sets = data.envSets.filter((one) => matches(needle, one.name, one.description, ...one.keys.map((key) => key.name)));
  const repoTotal = suggestions.workflows.length + suggestions.prompts.length + suggestions.env_sets.length;
  const personalTotal = data.workflows.length + data.prompts.length + data.envSets.length;
  const shownTotal = tab === "repo" ? repoWorkflows.length + repoPrompts.length + repoSets.length : workflows.length + prompts.length + sets.length;
  const total = tab === "repo" ? repoTotal : personalTotal;

  const copyOne = async (entry: { file: SuggestionFile; index: number }) => {
    if (copying.current) return;
    copying.current = true;
    setBusyCopy(true);
    setNotice("");
    try {
      await libraryApi().copySuggestion(projectId, entry.file, entry.index);
      setNotice("Copied to Personal. It is on this computer only; nothing was written to the repository.");
      setTab("personal");
      reload();
    } catch (failure) {
      const known = failure instanceof ApiFailure ? new Map([["name_taken", "Personal already has an item with that name. Rename or delete it first."], ["library_limit", "The library is full for this kind. Delete one to add another."], ["folder_untrusted", "This folder is not trusted yet."]]).get(failure.code) : undefined;
      setNotice(known ?? `Couldn’t copy it. ${failure instanceof ApiFailure ? plain(failure.message, 160) : "Something went wrong."}`);
    } finally {
      copying.current = false;
      setBusyCopy(false);
    }
  };

  // Export is one toolbar button and no screen of its own: the service builds the document, the browser saves it, nothing is written.
  // A multi-click gesture saves one file: only its first click counts, and `exporting` covers an export still pending.
  const exporting = useRef(false);
  const [busyExport, setBusyExport] = useState(false);
  const exportAll = async () => {
    if (exporting.current) return;
    exporting.current = true;
    setBusyExport(true);
    setNotice("");
    try {
      const document = await libraryApi().exportLibrary();
      saveBlob(exportFileName(document.exported_at), new Blob([JSON.stringify(document, null, 2)], { type: "application/json" }));
      setNotice("Exported the library to a JSON file. It holds every kind; an env set goes by key name only, never a value.");
    } catch (failure) {
      const older = failure instanceof ApiFailure && (failure.status === 404 || failure.code === "not_found");
      setNotice(older ? "This service has no library export. Update Orbyn, then try again." : `Couldn’t export the library. ${failure instanceof ApiFailure ? plain(failure.message, 160) : "Something went wrong."}`);
    } finally {
      exporting.current = false;
      setBusyExport(false);
    }
  };

  const row = (origin: Tab, kind: Kind, key: string, name: string, line: string) => (
    <li key={`${origin}:${kind}:${key}`}>
      <button type="button" className="lib-row" aria-pressed={pick?.origin === origin && pick.kind === kind && pick.key === key} onClick={() => { setPick({ origin, kind, key }); setNotice(""); }}>
        <span className="lib-name">{plain(name, 60)}</span>
        <span className="lib-line">{plain(line, 120)}</span>
      </button>
    </li>
  );
  const section = (kind: Kind, rows: React.ReactNode[]) => (rows.length === 0 ? null : (
    <div className="lib-group" key={kind}><h3 className="lib-group-title">{KIND_TITLE[kind]} · {rows.length}</h3><ul className="lib-list">{rows}</ul></div>
  ));

  const from = (source: unknown) => { const label = sourceLabel(source); return label ? `${label} · ` : ""; };
  const personal = tab === "personal" ? [
    section("workflow", workflows.map((one) => row("personal", "workflow", one.id, one.name, `${from(one.source)}${one.description || "No description"} · ${count(one.params.length, "parameter")}`))),
    section("prompt", prompts.map((one) => row("personal", "prompt", one.id, one.name, `${from(one.source)}${one.description || "No description"} · ${count(one.params.length, "parameter")}`))),
    section("env_set", sets.map((one) => row("personal", "env_set", one.id, one.name, `${count(one.keys.length, "variable")}${one.status === "planned" ? " · Planned" : ""}`))),
  ] : [];
  const repo = tab === "repo" ? [
    section("workflow", repoWorkflows.map((one) => row("repo", "workflow", refKey(one), one.name, `${one.description || "No description"} · ${count(one.params.length, "parameter")}`))),
    section("prompt", repoPrompts.map((one) => row("repo", "prompt", refKey(one), one.name, `${one.description || "No description"} · ${count(one.params.length, "parameter")}`))),
    section("env_set", repoSets.map((one) => row("repo", "env_set", refKey(one), one.name, count(one.keys.length, "variable")))),
  ] : [];

  const selected = (() => {
    if (!pick) return null;
    const personalAt = pick.origin === "personal";
    if (pick.kind === "workflow") return personalAt ? (data.workflows.find((one) => one.id === pick.key) ?? null) : (suggestions.workflows.find((one) => refKey(one) === pick.key) ?? null);
    if (pick.kind === "prompt") return personalAt ? (data.prompts.find((one) => one.id === pick.key) ?? null) : (suggestions.prompts.find((one) => refKey(one) === pick.key) ?? null);
    return personalAt ? (data.envSets.find((one) => one.id === pick.key) ?? null) : (suggestions.env_sets.find((one) => refKey(one) === pick.key) ?? null);
  })();

  const page = pick && selected ? (() => {
    const repoItem = pick.origin === "repo";
    const copy = () => { if (repoItem) void copyOne(selected as RepoWorkflow | RepoPrompt | RepoEnvSet); };
    if (pick.kind === "workflow") {
      const one = selected as Workflow | RepoWorkflow;
      return <WorkflowPage workflow={one} repo={repoItem} sets={data.envSets} hasThread busyCopy={busyCopy} onCopy={copy}
        onUse={(mode) => setOpen({ what: "use", mode, kind: "workflow", item: one as Workflow })} onEdit={() => setOpen({ what: "form", kind: "workflow", item: one as Workflow })} onDelete={() => setOpen({ what: "delete", kind: "workflow", id: (one as Workflow).id, name: one.name })} />;
    }
    if (pick.kind === "prompt") {
      const one = selected as Prompt | RepoPrompt;
      return <PromptPage prompt={one} repo={repoItem} busyCopy={busyCopy} onCopy={copy}
        onUse={(mode) => setOpen({ what: "use", mode, kind: "prompt", item: one as Prompt })} onEdit={() => setOpen({ what: "form", kind: "prompt", item: one as Prompt })} onDelete={() => setOpen({ what: "delete", kind: "prompt", id: (one as Prompt).id, name: one.name })} />;
    }
    const one = selected as EnvSet | RepoEnvSet;
    return <EnvSetPage set={one} repo={repoItem} busyCopy={busyCopy} onCopy={copy} onChanged={reload}
      onEdit={() => setOpen({ what: "form", kind: "env_set", item: one as EnvSet })} onDelete={() => setOpen({ what: "delete", kind: "env_set", id: (one as EnvSet).id, name: one.name })} />;
  })() : null;

  return (
    <section className="lib" aria-label="Library">
      <div className="lib-head" role="tablist" aria-label="Library">
        <button type="button" role="tab" aria-selected={tab === "repo"} className="lib-tab" onClick={() => setTab("repo")}>From repo · {repoTotal}</button>
        <button type="button" role="tab" aria-selected={tab === "personal"} className="lib-tab" onClick={() => setTab("personal")}>Personal · {personalTotal}</button>
        <span className="lib-spacer" />
        <Button small tone="quiet" icon={<Download size={14} aria-hidden />} busy={busyExport} onClick={(event) => { if (event.detail <= 1) void exportAll(); }}>Export</Button>
        <Button small icon={<Plus size={14} aria-hidden />} onClick={() => setOpen({ what: "form", kind: null, item: null })}>New item</Button>
      </div>
      <label className="lib-search">
        <Search size={14} aria-hidden />
        <input type="search" className="input" aria-label="Search the Library" placeholder="Search the Library" autoComplete="off" value={query} onChange={(event) => setQuery(event.target.value)} onKeyDown={(event) => { if (event.key === "Escape" && query !== "") { event.stopPropagation(); setQuery(""); } }} />
      </label>
      {needle ? <p className="lib-count" role="status">{shownTotal} of {total} items match “{plain(query.trim(), 40)}” <button type="button" className="lib-link" onClick={() => setQuery("")}>Clear search</button></p> : null}
      <p className="lib-note">{tab === "repo" ? <>Read from <code>.orbyn/</code> in this project’s committed files · read-only, shared through Git</> : <>Only on this computer · not in the repo</>}</p>
      {notice ? <p className="lib-notice" role="status">{notice}</p> : null}
      {tab === "repo" && repoRead.untrusted ? <p className="field-hint" role="note">This folder is not trusted yet, so its files are not read. Trust it first.</p> : null}
      {tab === "repo" && repoRead.error ? <p className="field-error" role="alert">Couldn’t read the repository’s files. {plain(repoRead.error.message, 160)}</p> : null}
      {tab === "repo" && suggestions.problems.length > 0 ? (
        <details className="lib-problems"><summary>Skipped from .orbyn/: {count(suggestions.problems.length, "entry", "entries")}</summary>
          <ul>{suggestions.problems.map((problem, at) => <li key={at}>{plain(problem.file, 40)} #{Number.isFinite(problem.index) ? problem.index : "?"}: {plain(problem.code, 40)}</li>)}</ul>
        </details>
      ) : null}
      {total === 0 && !needle ? (
        <div className="lib-empty">
          <b>{tab === "repo" ? "No team items in this repo" : "Nothing saved on this computer"}</b>
          <p className="field-hint">{tab === "repo" ? <>Orbyn shows the workflows, prompts and env sets your team already keeps in <code>.orbyn/</code>. It reads them; it does not create or change anything in the repo. Save your own in Personal.</> : "Save a command you repeat, a prompt you reuse or a set of variables. They are not in the repo and stay on this computer."}</p>
          <Button onClick={() => setOpen({ what: "form", kind: null, item: null })}>{tab === "repo" ? "New personal item" : "New item"}</Button>
        </div>
      ) : (
        <div className="lib-body">
          <div className="lib-lists">{tab === "repo" ? repo : personal}</div>
          {page ? <div className="lib-detail">{page}</div> : null}
        </div>
      )}
      {open?.what === "form" ? (
        <ItemForm kind={open.kind === null ? null : open.kind} item={open.item} library={data}
          onSaved={(name) => { setOpen(null); setTab("personal"); setNotice(`Saved ${plain(name, 40)} to Personal.`); reload(); }}
          onReload={() => { setOpen(null); setNotice("The list is up to date. Open the item again to see what changed."); reload(); }}
          onClose={() => setOpen(null)} />
      ) : null}
      {open?.what === "use" ? (
        <UseDialog item={open.item} kind={open.kind} mode={open.mode} where={{ thread_id: threadId }}
          onRun={() => undefined}
          onInsert={(mode, text) => place(threadId, mode, text, typeIntoTerminal)}
          onClose={() => setOpen(null)} />
      ) : null}
      {open?.what === "delete" ? (
        <DeleteItem kind={open.kind} id={open.id} name={open.name}
          onDone={() => { setOpen(null); setPick(null); setNotice(`Deleted ${plain(open.name, 40)}.`); reload(); }}
          onClose={() => setOpen(null)} />
      ) : null}
    </section>
  );
}
