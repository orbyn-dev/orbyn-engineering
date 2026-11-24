import { ArrowUp, Download, Folder, GitBranch, Plus, Search, SquareCheck } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { api, readPreferences } from "../api";
import type { Project } from "../../../api/types";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { navigate, routeHref } from "../../../product/state/route";
import { useWorkspace } from "../../../product/state/workspace";
import { Button } from "../../../ui/controls";
import { Dialog } from "../../../ui/Dialog";
import { FolderBrowser } from "../../../product/ui/FolderBrowser";
import { dropTrailingSlash, expandHome, homePrefix, pathName, replaceHomePrefix, shortenHome, splitPath, withSlash } from "./pathInput";
import { trackClone } from "../app-chrome/CloneJobs";
import { canPickFolder, checkCloneSource, pickFolder } from "../../repos";
import "../addProject.css";

export type AddProjectView = "folders" | "new" | "clone";

/** An `owner/name` on GitHub is a clone address too; anything else goes to Git as typed. */
export function cloneAddress(text: string): string {
  const typed = text.trim();
  return /^[\w.-]+\/[\w.-]+$/.test(typed) && !/^\.{1,2}\//.test(typed) ? `https://github.com/${typed}.git` : typed;
}

/** A folder this dialog hands Git as typed: a drive path, home, the current folder or a UNC share. */
const isLocalPath = (address: string) => /^(?:[A-Za-z]:[\\/]|\\\\|~[\\/]|\/|\.{1,2}[\\/])/.test(address);

/**
 * Why the typed clone source cannot be sent, or null when it can. The Add project flow hands Git any
 * source it reads: an http(s), ssh or file URL, an scp-like address, an owner/name on GitHub or a local
 * path, so only what cannot be a source at all is refused. The Repositories dialog's check supplies its
 * empty-address rule and its space-or-control-character one, which this flow applies to an address but
 * not to a local path, spaces and all (Git takes a folder with spaces); its https-or-ssh-only rule is
 * that page's route, not this flow's.
 */
export function cloneProblem(text: string): string | null {
  const address = cloneAddress(text);
  const checked = checkCloneSource(address);
  const local = isLocalPath(address);
  // eslint-disable-next-line no-control-regex
  const bad = /[\u0000-\u001f\u007f]/.test(address) || (!local && /\s/.test(address));
  if (!checked.ok && (address === "" || bad)) return checked.reason;
  // A scheme with nothing after it names no host and no path, so there is nothing to clone.
  if (!local && /^[A-Za-z][A-Za-z0-9+.-]*:\/\/*$/.test(address)) return "Enter the address to clone from.";
  return null;
}

/** The base directory setting, when the person has chosen one. */
function useBaseDirectory(): string | null {
  const preferences = useAsync("add-project-base-directory", (signal) => readPreferences(signal));
  const value = (Array.isArray(preferences.data) ? preferences.data : []).find((view) => view.key === "add_project_base_directory")?.value;
  return typeof value === "string" && value.trim() ? value : null;
}

function useFinish(onClose: () => void, onAdded?: (project: Project) => void) {
  const putProject = useWorkspace((state) => state.putProject);
  const refreshThreads = useWorkspace((state) => state.refreshThreads);
  const refreshManaged = useWorkspace((state) => state.refreshManaged);
  return (project: Project) => {
    putProject(project);
    void refreshThreads(project.id);
    void refreshManaged();
    // Where the add is one step of a longer flow, the caller keeps the person where they are.
    if (onAdded) {
      onAdded(project);
      return;
    }
    onClose();
    navigate(
      project.trusted
        ? { name: "new", projectId: project.id }
        : { name: "trust", projectId: project.id, threadId: null },
    );
  };
}

function Folders({
  onNew,
  onClone,
  onBrowse,
  onClose,
  added,
  onAdded,
}: {
  onNew(): void;
  onClone(): void;
  /** Opens the in-app folder browser at `from`, the folder the palette field names (null when it is not known yet). */
  onBrowse(from: string | null): void;
  onClose(): void;
  added: readonly Project[];
  onAdded?(project: Project): void;
}) {
  const finish = useFinish(onClose, onAdded);
  const [home, setHome] = useState<string | null>(null);
  const [value, setValue] = useState("~/");
  const [active, setActive] = useState(-1);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    const abort = new AbortController();
    api.browse(null, abort.signal).then(
      (listing) => setHome(listing.path),
      (failure: unknown) => {
        if (!abort.signal.aborted) setError(toFailure(failure).message);
      },
    );
    return () => abort.abort();
  }, []);

  const { dir, leaf } = splitPath(value);
  const expanded = home === null ? null : expandHome(dir, home);
  // The `~` root is shown in the home folder's own style, so a Windows field reads `~\` and not `~/`.
  useEffect(() => {
    if (home === null) return;
    setValue((current) => (current === "~/" || current === "~\\" ? homePrefix(home) : current));
  }, [home]);
  const listing = useAsync(`palette-browse:${expanded ?? ""}`, (signal) =>
    expanded === null ? Promise.resolve(null) : api.browse(expanded, signal),
  );
  const entries = useMemo(() => {
    const needle = leaf.toLowerCase();
    return (listing.data?.entries ?? []).filter((entry) => entry.name.toLowerCase().includes(needle));
  }, [listing.data, leaf]);
  const parent = listing.data?.parent ?? null;
  const folderRows = entries.length + (parent ? 1 : 0);
  // A folder with no subfolders says so, whether or not it also has a row going back up.
  const noFolders = listing.status === "ready" && listing.data !== null && entries.length === 0;
  const actions = [
    { key: "new", run: onNew },
    { key: "clone", run: onClone },
    { key: "browse", run: () => void browse() },
  ];
  const rows = folderRows + actions.length;

  const enter = (path: string) => {
    setValue(withSlash(home ? shortenHome(path, home) : path));
    setActive(-1);
    setError(null);
  };
  // In the desktop app the native folder dialog is the browser: it reaches every drive the system shows. What it picks only fills
  // the field, so adding is still the person's Enter; cancelling leaves the field and the location as they were. A plain browser
  // tab has no dialog, so it gets the in-app browser, opened at the folder the field already names.
  const browse = async () => {
    if (!canPickFolder()) {
      onBrowse(expanded);
      return;
    }
    if (busy) return;
    setError(null);
    try {
      const chosen = await pickFolder();
      if (chosen !== null) enter(chosen);
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : "The folder dialog did not open. Type the path instead.");
    }
  };
  const add = async (path: string) => {
    setBusy(true);
    setError(null);
    try {
      finish(await api.addProject(path));
    } catch (failure) {
      setError(toFailure(failure).message);
      setBusy(false);
    }
  };
  const addTyped = () => {
    const typed = value.trim();
    if (busy || (home === null && typed.startsWith("~"))) return;
    void add(dropTrailingSlash(expandHome(typed, home ?? "")));
  };
  const folderAt = (at: number) => {
    if (parent && at === 0) return parent;
    return entries[at - (parent ? 1 : 0)]?.path ?? null;
  };
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "o") {
      event.preventDefault();
      event.stopPropagation();
      void browse();
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      setActive(Math.min(rows - 1, Math.max(-1, active + (event.key === "ArrowDown" ? 1 : -1))));
    } else if (event.key === "Enter") {
      event.preventDefault();
      if (active >= folderRows) actions[active - folderRows]?.run();
      else if (active >= 0) {
        const path = folderAt(active);
        if (path && (event.metaKey || event.ctrlKey)) void add(path);
        else if (path) enter(path);
      } else addTyped();
    }
  };
  const isMac = /mac/i.test(navigator.platform);

  return (
    <>
      <div className="palette-search">
        <Folder size={16} aria-hidden />
        <input
          autoFocus
          className="add-project-path"
          aria-label="Folder path"
          placeholder="Enter a folder path"
          value={value}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          onChange={(event) => {
            setValue(replaceHomePrefix(event.target.value));
            setActive(-1);
            setError(null);
          }}
          onKeyDown={onKeyDown}
        />
      </div>
      {error ? (
        <p className="palette-error" role="alert">
          {error}
        </p>
      ) : null}
      <div className="palette-list">
        {listing.status === "error" ? <p className="sidebar-empty">{listing.error.message}</p> : null}
        {added.length > 0 ? (
          <>
            <div className="palette-group">Added</div>
            {added.map((project) => (
              <div key={project.id} className="add-project-added">
                <SquareCheck size={14} aria-hidden />
                <span className="palette-title add-project-path-row">{home ? shortenHome(project.path, home) : project.path}</span>
                <span className="add-project-hint">{project.is_git ? "Git" : "Folder"}</span>
              </div>
            ))}
            <div className="add-project-sep" role="separator" />
          </>
        ) : null}
        {folderRows > 0 ? <div className="palette-group">Folders</div> : null}
        {parent ? (
          <button type="button" data-active={active === 0 || undefined} onMouseMove={() => setActive(0)} onClick={() => enter(parent)}>
            <ArrowUp size={16} aria-hidden />
            <span className="palette-title">{pathName(parent)}</span>
          </button>
        ) : null}
        {noFolders ? <p className="sidebar-empty">{leaf ? "No results" : "No folders here"}</p> : null}
        {entries.map((entry, at) => {
          const row = at + (parent ? 1 : 0);
          return (
            <button
              key={entry.path}
              type="button"
              data-active={active === row || undefined}
              onMouseMove={() => setActive(row)}
              onClick={() => enter(entry.path)}
            >
              {entry.is_git ? <GitBranch size={16} aria-hidden /> : <Folder size={16} aria-hidden />}
              <span className="palette-title add-project-path-row">{home ? shortenHome(entry.path, home) : entry.path}</span>
              <span className="add-project-hint" aria-hidden>
                {entry.is_git ? "Git" : "Folder"}
              </span>
            </button>
          );
        })}
        <div className="add-project-sep" role="separator" />
        {[
          { icon: <Plus size={16} aria-hidden />, title: "New project", hint: "README and first commit" },
          { icon: <Download size={16} aria-hidden />, title: "Clone a repository", hint: "Git URL or owner/name" },
          { icon: <Search size={16} aria-hidden />, title: "Browse this computer", hint: isMac ? "⌘O" : "Ctrl O" },
        ].map((action, at) => (
          <button
            key={action.title}
            type="button"
            data-active={active === folderRows + at || undefined}
            onMouseMove={() => setActive(folderRows + at)}
            onClick={actions[at]?.run}
          >
            {action.icon}
            <span className="palette-title">{action.title}</span>
            <span className="add-project-hint add-project-hint-prose">{action.hint}</span>
          </button>
        ))}
      </div>
    </>
  );
}

/** One field, Cancel and the confirm: the design's New project and Clone dialogs. */
function AddDialog({
  title,
  label,
  confirm,
  mono,
  hint,
  check,
  submit,
  onCancel,
}: {
  title: string;
  label: string;
  confirm: string;
  mono?: boolean;
  hint: ReactNode;
  /** What is wrong with the typed value, or null when it may be sent: an answer keeps the dialog open and sends nothing. */
  check?(value: string): string | null;
  submit(value: string): Promise<void>;
  onCancel(): void;
}) {
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const field = useRef<HTMLInputElement>(null);
  const live = useRef(true);
  useEffect(
    () => () => {
      live.current = false;
    },
    [],
  );
  const send = async () => {
    const text = value.trim();
    if (!text || busy) return;
    const problem = check?.(text) ?? null;
    if (problem !== null) {
      // The dialog answers with the reason and stays open; nothing is sent and the field takes
      // the focus back, so the address can be fixed where the message points.
      setError(problem);
      field.current?.focus();
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await submit(text);
    } catch (failure) {
      if (live.current) setError(toFailure(failure).message);
    } finally {
      if (live.current) setBusy(false);
    }
  };
  return (
    <Dialog
      title={title}
      onClose={onCancel}
      busy={busy}
      className="add-project-dialog"
      footer={
        <>
          <Button onClick={onCancel} disabled={busy}>
            Cancel
          </Button>
          <Button tone="primary" busy={busy} disabled={!value.trim()} onClick={() => void send()}>
            {confirm}
          </Button>
        </>
      }
    >
      <form
        className="add-project-form"
        onSubmit={(event) => {
          event.preventDefault();
          void send();
        }}
      >
        <label htmlFor="add-project-field">{label}</label>
        <input
          id="add-project-field"
          ref={field}
          className={mono ? "dialog-field dialog-field-mono" : "dialog-field"}
          data-dialog-focus
          value={value}
          autoComplete="off"
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          onChange={(event) => {
            setValue(event.target.value);
            if (error !== null) setError(null);
          }}
        />
        {error ? (
          <p className="field-error" role="alert">
            {error}
          </p>
        ) : hint ? (
          <span className="add-project-note">{hint}</span>
        ) : null}
      </form>
    </Dialog>
  );
}

/** Adding a project: a path to add in place, a new empty folder or a clone. Folders stay where they are. */
export function AddProjectPalette({
  initialView = "folders",
  onClose,
  onSubdialog,
  added = [],
  onAdded,
}: {
  initialView?: AddProjectView;
  /** Projects this flow has already added, listed under "Added". */
  added?: readonly Project[];
  /** Keeps the person in place after an add, instead of opening the project. */
  onAdded?(project: Project): void;
  onClose(): void;
  /** True while a dialog sits over the palette, which hides itself meanwhile. */
  onSubdialog(open: boolean): void;
}) {
  const [view, setView] = useState<AddProjectView>(initialView);
  const [browsing, setBrowsing] = useState(false);
  const [browseFrom, setBrowseFrom] = useState<string | null>(null);
  const [browseError, setBrowseError] = useState<string | null>(null);
  const finishAdd = useFinish(onClose, onAdded);
  // A dialog that added something is done; where the flow goes on, the palette is what is left.
  const finish = (project: Project) => {
    setView("folders");
    setBrowsing(false);
    finishAdd(project);
  };
  const base = useBaseDirectory();
  const home = useAsync("add-project-home", (signal) => api.browse(null, signal)).data?.path ?? null;
  const sub = view !== "folders" || browsing;
  useEffect(() => {
    onSubdialog(sub);
    return () => onSubdialog(false);
  }, [sub, onSubdialog]);
  // Closing a dialog opened from outside the palette (the empty home) ends the add flow.
  const cancel = () => (initialView === "folders" ? setView("folders") : onClose());
  return (
    <>
      <Folders onNew={() => setView("new")} onClone={() => setView("clone")} onBrowse={(from) => { setBrowseFrom(from); setBrowsing(true); }} onClose={onClose} added={added} onAdded={onAdded} />
      {view === "new" ? (
        <AddDialog
          title="New project"
          label="Name"
          confirm="Create project"
          hint={
            <>
              {base ? (
                <>
                  Created in <span className="mono">{base}</span>, your base directory.{" "}
                  <a className="add-project-link" href={routeHref({ name: "settings", section: "general" })} onClick={onClose}>
                    Change
                  </a>
                  <br />
                </>
              ) : null}
              Creates a folder with a README and a first commit, so worktrees work.
            </>
          }
          submit={async (name) =>
            // With a base directory chosen the folder is made there; without one the service picks.
            finish(await api.createProject(name, base && home ? `${dropTrailingSlash(expandHome(base, home))}/${name}` : undefined))
          }
          onCancel={cancel}
        />
      ) : null}
      {view === "clone" ? (
        <AddDialog
          title="Clone a repository"
          label="Git URL or owner/name"
          confirm="Clone"
          mono
          hint={
            <>
              Any Git URL works, or an owner/name on GitHub.{" "}
              {base ? (
                <>
                  It is cloned into <span className="mono">{base}</span>;{" "}
                </>
              ) : (
                "It is cloned into your base directory; "
              )}
              progress shows in the sidebar until it finishes.
            </>
          }
          check={cloneProblem}
          submit={async (url) => {
            const job = await api.startClone(cloneAddress(url));
            if (onAdded) setView("folders");
            else onClose();
            trackClone(job, finish);
          }}
          onCancel={cancel}
        />
      ) : null}
      {browsing ? (
        <Dialog title="Browse this computer" onClose={() => setBrowsing(false)} className="add-project-dialog">
          {browseError ? (
            <p className="field-error" role="alert">
              {browseError}
            </p>
          ) : null}
          <FolderBrowser
            start={browseFrom}
            actionLabel="Add this folder"
            onChoose={(path) => {
              setBrowseError(null);
              api.addProject(path).then(finish, (failure: unknown) => setBrowseError(toFailure(failure).message));
            }}
          />
        </Dialog>
      ) : null}
    </>
  );
}
