# Multi-repository projects

A project can hold more than one Git repository, and one thread can work in several of them at once, each on
its own branch. Beta: behind a flag; with the flag off every route here answers `404` and the web app hides
the repository list. Field names follow the T3 Code workspace folder schema (MIT, `9168ca73d`, names only) in
snake_case; the web side's guessed contract, `web/src/features/repos/contract.ts`, is adopted except for the
differences in the last section. The Rust types live in `orbyn-contracts` and generate the web types.

## Model

- A **project** keeps its own folder, which is its first repository (`id` equal to the project id,
  `primary: true`). More repositories are rows of the project (`project_repos`: id, project, path, trusted,
  managed, clone state). A repository belongs to one project; a folder that is already another project's
  repository is refused (`repo_exists`).
- **Trust is per repository.** An untrusted repository is listed so it can be trusted, and it can do
  nothing else: it cannot be added to a thread, started in, cloned into or published (`repo_untrusted`,
  409). Nothing runs in a folder before the person trusts that folder.
- A **multi-repository thread** gets a thread folder under app data, `threads/<thread_id>/`, with one Git
  worktree per repository inside it, `<repo name>/`, each on a new branch of its own cut from the branch the
  person chose. The engine's working directory is the thread folder, which is not a repository. The thread's
  write scope is the set of its worktrees; an approval names the worktree path as every approval does today.
  A single-repository thread is unchanged and has no thread folder.
- At most **8** repositories per thread (`max_per_thread`).
- **Git actions are per repository**: stage, commit, push, pull request, branch and the Changes list take a
  `repo_id`, default the project's primary repository, so a one-repository thread sees no change. A commit
  or push never spans repositories. See "Per-repository Git routes" below.
- Removing a repository from a project stops tracking it. Files, branches and the threads that used it stay;
  a thread that still has a worktree of it keeps working.

## Routes

`{id}` is the project id: the selected checkout's `members[0].project_id`. Reads are not Person-class. Every
write is, and goes in `ENFORCED` and `PERSON_ROUTES` (`personKey.ts`).

| Route | Person | Answers |
|---|---|---|
| `GET /api/projects/{id}/repos` | no | `ReposView`; `404` when the flag is off |
| `POST /api/projects/{id}/repos/add-folder` | **yes** | `RepoRow`. Body `{ path }`. In place; nothing is copied |
| `POST /api/projects/{id}/repos/create` | **yes** | `RepoRow`. Body `{ name }`. A new folder under `clone_directory`, then `git init` |
| `POST /api/projects/{id}/repos/clone` | **yes** | `RepoRow` with `clone.state: "cloning"`. Body `{ source }`; runs in the background |
| `POST /api/projects/{id}/repos/{repo_id}/publish` | **yes** | `RepoRow`. Body `PublishInput` |
| `POST /api/projects/{id}/repos/{repo_id}/trust` | **yes** | `RepoRow` |
| `POST /api/projects/{id}/repos/{repo_id}/setup-git` | **yes** | `RepoRow`. `git init` in a folder that has none |
| `POST /api/projects/{id}/repos/{repo_id}/remove` | **yes** | `204` |
| `POST /api/projects/{id}/repos/{repo_id}/clone/cancel` | **yes** | `RepoRow`; stops a running clone and removes its partial folder |
| `POST /api/projects/{id}/repo-threads` | **yes** | `Thread`. Body `StartThreadInput` |
| `GET /api/threads/{id}/repos` | no | `ThreadRepo[]`; `404` when the thread is not a multi-repository thread |
| `POST /api/threads/{id}/repos` | **yes** | `ThreadRepo`. Body `{ repo_id }`; adds a trusted repository of the project |

### Per-repository Git routes

No new Git routes. The thread routes that exist take an optional `repo_id`, in the JSON body for a `POST` and
in the query for a `GET`; absent means the primary repository, so a one-repository thread is unchanged:

- `POST /api/threads/{id}/git/action` (stage, commit, push, create a branch and the rest of its actions): body
  `repo_id`. The action runs in that repository's worktree only.
- `GET /api/threads/{id}/commits` and the Changes reads: query `repo_id`; each changed file in the answer
  carries its `repo_id` and a repo-relative path.
- `POST /api/threads/{id}/pulls` (link or open a pull request), `.../pulls/{number}/merge` and `.../review`:
  `repo_id` says which repository's pull request; a thread may link one pull request per repository.

- **File reads and writes name their repository.** The web app's hunk revert, its undo and its inline line
  edit are all one read and one write of a file: `GET /api/threads/{id}/files/content?path=` and
  `PUT /api/threads/{id}/files/content` with `{ path, content, base_revision }`. There is no separate hunk-revert
  route. Both take `repo_id` (query on the read, body on the write); absent means the primary repository. The
  service resolves `path` against that repository's worktree only and refuses a path that does not belong to it
  (`400 invalid`: absolute, `..`, a symlink out of the worktree), so a same-named file in another repository is
  never read or written. The `revision` a read returns belongs to that repository's file, so a write with
  another repository's revision is a conflict. A `repo_id` that is not one of the thread's repositories is
  `404 not_found`; an untrusted one is `409 repo_untrusted`. The write stays Person-class.

A `repo_id` that is not one of the thread's repositories is `404 not_found`; one whose repository is no longer
trusted is `409 repo_untrusted`. These routes keep the class they have today (the Person-class ones stay
Person-class; push stays as the existing route classifies it). Removing a repository from a thread is not in
the beta: a thread keeps its worktrees, and `.../repos/{repo_id}/remove` above removes it from the project only.

Repository settings (default branch, setup and cleanup scripts) are not in the beta: they are the project's
settings today, applied to every repository. Per-repository settings are Planned.

## Shapes

```ts
type RepoRow = { id; name; path; is_git; trusted; branch: string | null; branches: string[]; has_remote: boolean;
                 primary: boolean;
                 clone: { state: "cloning" | "failed"; percent: number | null; message?: string | null } | null };
type ReposView = { repositories: RepoRow[]; clone_directory: string | null; max_per_thread: number };
type PublishInput = { name: string /* owner/name */; visibility: "private" | "public" };
type StartThreadInput = { repos: { repo_id: string; base_branch: string }[]; engine: string;
                          model: string | null; permission_mode: PermissionMode; message?: string };
type ThreadRepo = { repo_id; name; branch; added: number; deleted: number; trusted: boolean;
                    files?: string[] };
```

- `ThreadRepo.files`: **confirmed.** The paths of this repository's changed files, relative to the
  repository root. A change in the Changes list carries `repo_id` and a repo-relative path, so two
  repositories can each have a `README.md`. Absent when the service has not read the changes yet.
- `StartThreadInput.permission_mode`: **confirmed.** The thread is created in that mode, checked against the
  engine's `permission_modes`; a mode the engine does not offer is `400 permission_mode_unsupported`
  (Cursor refuses what it cannot enforce, as for a single-repository thread). `model` of `null` is the
  engine's default. `message` is the optional first message.
- `clone_directory` is under app data (`repos/`); `null` until the service has made it.
- A repository being cloned has `clone` set and `branches: []`; a failed clone stays in the list with
  `clone.state: "failed"` and a message until it is removed or the clone is retried by a new `clone`.

## Task folders, folder scan and workspace import

All three are in T3 Code at `9168ca73d` (`workspaceFolders.ts`: `TaskFolder`, `WorkspaceScanFoldersInput`,
`TASK_FOLDER_DEFAULT_REPOSITORIES_MAX`); none is "not in the beta". Same flag as the rest of this document.

### Task folder

The thread folder in "Model" is the task folder. `StartThreadInput` adds an optional `branch_name`: when given,
every repository's worktree is cut on that new branch name (each from its own `base_branch`); absent, the
service makes one name and uses it in every repository, so one thread is one branch name across repositories.
Names follow the usual branch-name checks; a name already used in any chosen repository is `409 invalid`
and nothing is created. Between 2 and `max_per_thread` (8) repositories make a task folder; one repository is
the unchanged single-repository thread.

| Route | Person | Answers |
|---|---|---|
| `GET /api/threads/{id}/task-folder` | no | `TaskFolder`; `404` when the thread has no task folder |

```ts
type TaskFolder = { root: string; branch: string; repos: ThreadRepo[]; max_per_thread: number };
```

`root` is shown as text only. Each `ThreadRepo` is also the worktree's name inside `root` (`name`, a safe
folder name), so a repository's worktree is `<root>/<name>`. The web app reads the task folder, never a
field of `Thread`. Adding a repository later is the existing `POST /api/threads/{id}/repos`: it cuts the
thread's `branch` there from that repository's default branch and answers `repo_limit` (409) past 8.

### Folder scan

`POST /api/projects/{id}/repos/scan`, **Person-class** (it reads a folder the person picked; it writes
nothing). Body `{ path }`. The path is untrusted and is checked as `add-folder` checks it (rule 4 in "Clone and
path input"): it must exist, be a directory (or a `.code-workspace` file, see below), be canonicalized, not
be inside Orbyn's data directory. Answer:

```ts
type RepoScan = { path: string; is_git: boolean; code_workspace_file: string | null;
                  folders: ScannedFolder[]; truncated: boolean };
type ScannedFolder = { name: string; path: string; is_git: boolean; source: "child" | "code_workspace";
                       in_project: boolean; refused: "outside" | "missing" | "not_directory" | null };
```

- `child`: the directories one level under `path`. A symbolic link is not followed; a name that starts with a
  dot is skipped; at most 200 entries are looked at and 64 returned, then `truncated: true`. `is_git` is a
  `.git` directory or file in it. Nothing deeper than one level is read.
- `name` is the folder's own name; `path` is canonical. Both are untrusted text.
- `in_project` is true when the folder is already a repository of any project.
- Nothing is added and nothing is trusted. The person adds a result through `add-folder`, one call each
  (Person-class), and trusts it through `trust`.

### Workspace import (`.code-workspace`)

The same route. When `path` is a `.code-workspace` file, or a folder with exactly one at its top, the service
reads that file (at most 256 KiB, JSON with comments and trailing commas; anything else is `400 invalid`)
and returns its `folders` as `source: "code_workspace"` entries, `code_workspace_file` naming the file. An
entry is `{ path, name? }`, at most 64; each is resolved against the file's folder and canonicalized.
A folder that resolves outside the file's own folder, or whose path has a drive prefix, is listed with
`refused: "outside"` and cannot be added by import, so a workspace file cannot point the app at an arbitrary
place (the person can still pick that folder with `add-folder`). A folder that does not exist is `refused:
"missing"`; a file, `"not_directory"`. No setting in the file other than `folders` is read; tasks, launch
settings and extensions are ignored, and nothing in it is run. Listed folders are untrusted like every added
folder: they go to `add-folder` and then wait for `trust`.

## Errors

The usual `{ code, message }`. `repo_untrusted` (409), `repo_exists` (409), `repo_limit` (409: more than
`max_per_thread`), `clone_refused` (400), `not_git` (409: an action that needs Git on a folder that has
none), `permission_mode_unsupported` (400), `not_found` (404), `invalid` (400). A scan that finds nothing is
an answer with an empty `folders`, not an error.

## Clone and path input

Source text, a folder path and a repository name are all untrusted. The service checks every one again,
whatever the page checked.

1. A clone source is an `https://` URL, an `ssh://` URL or an scp-style `user@host:path`. Anything else is
   `clone_refused`: a source that starts with `-` (Git reads it as an option), `ext::`, `fd::`, `file://`,
   a local path, a URL with a newline or control character, or one with a user name and password in it.
   A local folder joins a project only through `add-folder`, never through `clone`.
2. Git is run with the source after `--`, `GIT_ALLOW_PROTOCOL=https:ssh`, `GIT_TERMINAL_PROMPT=0`,
   `protocol.ext.allow=never`, `protocol.file.allow=never`, no submodule recursion and no template hooks.
   A clone has a 10 minute limit and is cancellable.
3. The destination is chosen by the service, never the caller: a folder name from the source's last path
   segment (letters, digits, `-`, `_`, `.`; no leading dot; a number appended when taken) under
   `clone_directory`, canonicalized and checked to be inside it after every symlink is resolved. A name or
   path containing `..`, a separator or a drive prefix is refused.
4. `add-folder` takes a path the person picked: it must exist, be a directory, not be inside Orbyn's data
   directory, and not be another project's repository. Paths, names, branch names and remote URLs are shown
   as text only, never as a link, command or markup.

## As built: project repositories

Behind `CoreConfig::multi_repo` (`ORBYN_MULTI_REPO=1`): with it off every route below answers `404`. Built: the repository list,
`add-folder`, `create`, `clone`, `cancel`, `trust`, `setup-git`, `remove` and the folder scan with `.code-workspace` import. The
repository threads, the thread's repository routes, the task folder and `repo_id` on the Git and file routes are built too (see the
sections below). **Not built:** `publish` (it checks the repository is trusted and then answers `422 unsupported`: "Publishing a
repository arrives in a later update.").

- **Storage.** Table `project_repos` (migration 64): id, project, name, path (unique), trusted, is_git, managed, clone state and message.
  The project's own folder is not a row; it is the first row of the list (`primary`, `id` the project id), and `trust` and `setup-git` on it
  go to the project's own trust and Git init. `remove` on it is `400`.
- **Names.** A repository's `name` is a safe folder name (letters, digits, `-`, `_`, `.`; no leading dot; at most 64) taken from the
  folder or source, unique within the project (`-2`, `-3` appended). It is what a task folder's worktree will be called.
- **Trust.** `add-folder` and `clone` start untrusted; `create` starts trusted (Orbyn made an empty folder). `trust` is refused while a
  clone is running. `setup-git` on an untrusted folder is `409 repo_untrusted`. A folder that is any project's folder or any project's
  repository is `409 repo_exists`; one inside Orbyn's data, or the skills library, is `400`. A project holds at most 64 extra
  repositories (`409 repo_limit`).
- **Clone.** The source passes the rules in "Clone and path input" (`400 clone_refused`; the check runs before anything is stored). A clone
  runs in the background into `<data>/repos/.incoming-<id>`, then is renamed to `<data>/repos/<name>`, with `GIT_ALLOW_PROTOCOL=https:ssh`,
  `GIT_TERMINAL_PROMPT=0`, no helpers, no file protocol, no submodules, no template hooks and a 10 minute limit. The row answers
  `clone.state: "cloning"` at once and ends as no `clone` (done) or `failed` with a message of at most 300 characters; a failed clone stays
  listed until it is removed. `percent` is always `null`. `cancel` stops a running clone (`400` when none runs) and answers the row as
  failed, "Cancelled."; a clone running when the core stops is marked failed at the next start. A repository that is cloned still has to
  be trusted.
- **Scan.** As the section above, `RepoScan` and `ScannedFolder` in `orbyn-contracts`; at most 200 entries looked at, 64 returned; a
  workspace file over 256 KiB, not JSON (comments and trailing commas allowed) or without `folders` is `400`.
- **Errors added:** `repo_exists`, `repo_untrusted`, `repo_limit` (409), `clone_refused` (400), `not_git` (409; for the thread slice).

## As built: web UI

The web client for the project repository routes (`web/src/features/repos/`) uses the generated types (`RepoRow`, `ReposView`,
`RepoClone`, `RepoScan`, `ScannedFolder`) and was checked against `crates/orbynd/src/routes/repos.rs`. Real against the first guess:

- **Scan source** is `"child" | "code_workspace"` on the wire (snake_case, as every generated enum). The guess and this document's first
  draft said `code-workspace`; the page dropped such entries as unknown, so a workspace import listed nothing. Fixed in the client and above.
- **Clone** is `state: "cloning" | "failed"`; `percent` is optional and always `null` or absent, so a running clone shows "Cloning" with no
  figure or bar. A cancelled clone is `failed` with the message `Cancelled.`; `cancel` answers that row (not Git, untrusted, no branches),
  and the row stays listed until `remove`.
- **Trust.** `create` starts trusted with Git; `add-folder` and `clone` start untrusted. The service refuses `setup-git` on an untrusted
  folder (`409 repo_untrusted`) and `trust` while a clone is running, so the row's state is trust first: a folder that is not trusted
  is shown as Not trusted (with Trust…) whether or not it holds Git, and only a trusted folder with no Git offers Set up Git. A cloning or
  failed row offers no Trust. The Create dialog says it starts trusted; the Add a folder and Clone dialogs say they do not.
- **Publish** answers `422 unsupported` ("Publishing a repository arrives in a later update.") after the trust check, and takes no body.
  The dialog shows the Planned pill with the service's message on the repository step, leaves Publish off and shows no success.
- **Answers and codes.** `remove` is `204` with no body. A `404` on the list is "not offered": nothing is drawn, no error is shown and
  no other call is made. Error codes pass through as `{ code, message }`: `repo_exists`, `repo_limit`, `repo_untrusted` (409),
  `clone_refused`, `invalid` (400), `not_found` (404), `unsupported` (422).
- `StartThreadInput`, `TaskFolder`, `ThreadRepo` and the `repo_id` request fields are the generated types; the mock (`web/mock/repos.ts`, hidden behind the preview flag)
  follows the generated shapes and the service's answers above.

Thread repositories and `repo_id` on Git and file calls, real against the first guess (`ThreadRepo`, `repo_id`; closes #1046):

- **A diff file carries no `repo_id`.** The Changes panel assumed `GET .../diff` returned every repository's files, each marked with its
  repository. The service answers ONE repository's files per read (`?repo_id=`, absent is the primary) and `FileDiff` has no repository
  field. The panel now reads once per trusted repository of `GET .../repos` and marks each file itself; a thread with no repositories
  reads once, as before. An untrusted repository reports no changes (`files: null`) and its reads are `409 repo_untrusted`, so it is not
  read; if every read fails the Changes show the error.
- **`ThreadRepo.files` is `string[] | null`** (null for an untrusted repository); `pull_request` does not exist. The menu on a
  repository's group therefore offers Commit, Push and Create pull request only (`POST .../git/action` with `repo_id`). Open, Merge
  and Review pull request are gone until the pull request routes take `repo_id`; they would have acted on the primary repository's
  pull request number, and a `repo_id` sent to them was ignored.
- **Where `repo_id` goes** is as built above: query on the diff, commits, branches and file reads, body on the git action, branch writes
  and file write. `api.diff`, `api.branches`, `api.createBranch` and `api.switchBranch` now take it (they had none).
- **Hunk revert, undo and line edit** send `repo_id` on the read and the write; the revision of a repository's read goes back on a write
  to that same repository. A Markdown file shown rendered read the primary checkout's file at the same path; it now reads the file of its
  own repository.
- **Errors.** `409 repo_untrusted` is not a changed file: a hunk or line write shows the service's message, not "The file has changed";
  the file conflict is `409 file_conflict` and keeps its message. A git action on an untrusted repository opens the existing trust
  prompt; a `404` says it is not one of the thread's repositories. `GET .../repos` `404` is an ordinary thread (or the flag off): nothing
  draws and nothing is said.
- **Repository choice (header Git menu and pickers).** A thread in two or more repositories shows one chip, "Repository: <name>", beside the
  header Git button, in the branch dialog and, in the Commit and Branch scopes, under the Changes header. The choice is shared by those
  places for the thread (kept in memory, the primary by default: the repository whose id is the project's, else the first). Every read
  and write they make carries its `repo_id`: `GET .../branches` and `.../commits` in the query, `POST .../git/action`, `.../branches`
  and `.../branches/switch` in the body; the primary's calls leave it out. A commit or branch comparison is one repository's, so those
  two scopes read the diff of the chosen repository only. An ordinary thread (or one in a single repository) gets no chip and no call
  changes (no extra argument at all). If the chosen repository can't answer Git questions, the Git menu stays so another can be picked,
  with its Git items off. Names from the service are drawn as one capped line of text.
- **Trust from a refused write.** `409 repo_untrusted` on a hunk revert, undo or line edit adds a "Trust repository…" button to the
  notice, which opens the same trust dialog the repository's Git menu opens (one click, one write, busy guard). Trusting clears the
  notice and re-reads; the refused write is not repeated, the person tries again. It is offered for a multi-repository thread only.
- **Editor and file tabs of another repository's file.** A file tab can name a repository: its id is `["t", path, repo_id]` where a
  primary-repository or ordinary-thread tab is still `["t", path]` (byte for byte what older sessions saved; those restore as before).
  "Open" and "Open in editor" on a changed file of another repository open that tab, with the repo-relative path, instead of a notice.
  The read is `GET .../files/content?path=&repo_id=`, the write sends `repo_id` in the body with the revision of that repository's own
  read, and the editor session is keyed by repository and path, so the same path in two repositories is two files that never share
  text. The title line names the repository and the tab's tooltip reads `README.md (name)`. `409 file_conflict` keeps its Changed on
  disk banner (Keep mine, Reload); `409 repo_untrusted` on a read or a write shows the service's message and the existing trust
  dialog, keeps the text and repeats nothing; a path outside the repository (`400`) or a repository that is not the thread's (`404`) is
  said in words. That tab is the editor only: no rendered preview, no line comments, no "Add file to message", no outside editor and
  no absolute path, and images, archives, documents, binary and truncated files say they can't be shown yet, because those routes
  (`.../files/raw`, zip, office, download, `previews/file`, `open-in-editor`) take no `repo_id`.
- **Turn totals.** The turn picker's per-turn figures were the primary repository's alone. In a multi-repository thread each turn's
  totals sum `GET .../diff?scope=turn&run=` over the thread's trusted repositories, one read per repository with its own `repo_id`.
  An untrusted repository is not read and a repository whose read fails is left out, and the row says "N repo not counted" instead of
  counting them as zero changes; if no repository could be read the row has no figures. `ThreadRepo.added`, `deleted` and `files` are
  against `HEAD`, not per turn, so they are not used. An ordinary thread reads once, as before. Whether the service keeps a turn's
  checkpoint in a repository other than the primary is not known; a refusal there shows as "not counted".
- **The 200-file cap.** A repository's `ThreadRepo.files` is cut at 200 by the service. A repository group whose list holds 200
  says "200 shown" quietly on its header.
- **Still primary only.** Pull request screens, Publish repository (a quiet disabled item with a title when another repository is
  chosen) and the Create pull request base check of the Changes footer act on the primary repository.
- **Not verified against a live service.** Checked with the mock and spies on the client; the thread paths have not run against an
  `orbynd` with `ORBYN_MULTI_REPO=1`.

### As built: web UI (repo threads)

The new-thread dialog, the Repos popover and the client now use the generated `StartThreadInput`, `StartRepoChoice`, `AddThreadRepo`,
`TaskFolder` and `ThreadRepo`; this replaces "Still hand-written in `contract.ts`" above. Read against `routes/repos.rs` and
`orbyn-core/src/repos/threads.rs`. Real against the first guess:

- **Paths, methods, bodies and answers matched** (`POST .../repo-threads` answers the `Thread`; `POST /api/threads/{id}/repos` `{ repo_id }`
  answers `ThreadRepo`; `GET .../task-folder`). `StartThreadInput.message` and `branch_name` are `string | null` and sent only when set.
- **One repository is `400`, not a normal start.** The dialog allowed "Start in 1 repository". The service refuses fewer than two
  (`400 invalid`), so Start is off below two ticked repositories, with a note that one repository uses the normal New thread. Past the
  limit (checked first) is `409 repo_limit`; the dialog also stops ticking at `max_per_thread` from the list.
- **A taken branch name is `400 invalid`** ("<repo> already has a branch named <name>."), not `409`. A base branch that is not a local
  branch is `400 invalid` too, and a repository still cloning is `400`, not `repo_untrusted`.
- **Messages are the service's own and are shown as written**, with the dialog kept open: `repo_limit`, `not_git`, `invalid`,
  `not_found` and `permission_mode_unsupported` (this one with a short lead-in). `409 repo_untrusted` also reads the repository list again
  and offers "Trust <name>…" (the existing trust dialog, one click one write) for each chosen repository that list now shows as not trusted.
  Trusting does not start the thread; the person presses Start again. The client check (two to eight, a branch name by
  `git check-ref-format`) only keeps the button honest; the service decides.
- **Task folder.** The Repos popover shows `root` as text with a copy button, the one branch, and "N of `max_per_thread` repositories",
  read again after an add. `404` (ordinary thread, flag off) draws nothing. A start opens the new thread (`navigate`).
- **Add a repository** uses the popover's existing "Add a repository to this thread…" menu, which lists trusted, finished, Git repositories
  of the project that are not in the thread; the service's refusal (`repo_limit`, `repo_untrusted`, `not_git`, `400` already in the thread,
  a taken branch name, no branch checked out) shows as a notice and the list stays as it was. One click, one write, busy guard.
- **The thread's repository tree.** The left bar's Files view shows a thread in two or more repositories as one checkout row under its
  project's Worktrees, labelled with the thread's branch and marked as the thread's own. The repository chip above it (the explorer's own,
  sharing the choice of the header Git menu and the Changes panel: the thread's primary until the person picks another) names the
  repository shown. Its tree is read through `GET /api/threads/{id}/files?path=&repo_id=` (no `repo_id` for the primary, so it reads as
  the service's default). A file of the primary repository opens as the thread's own file tab, with no repository on the tab (the shape
  the rest of the app opens a thread's files with); a file of another repository opens as that repository's tab, whose read and write
  carry its `repo_id` rather than being resolved against the thread's task folder. A thread in one repository, and every ordinary
  thread, keeps the explorer it had: no chip, no checkout row and no call to that route. Known limits: the route answers no change
  letters (the service cuts a folder at 5,000 entries and its answer says nothing about it, so the level is shown as complete), no
  file watcher covers the thread's worktree, so the tree reads again when the project's data changes (a finished run, for one) or the
  person presses Refresh, and on re-entering the thread; the repository list is read again with the thread's Changes (a run that ended,
  a repository added or trusted), so one added to a running thread can be picked. A file the row lists opens its tab and no more: it is not dragged into the message and has no outside
  editor, "Reveal" or rendered preview, because the thread names one folder and these paths are repository-relative (the same limit the
  other repositories' file tabs have). `GET .../files/metadata` still has no web caller: no screen shows a file's metadata. No frame
  draws this row or the explorer's chip, so both reuse what exists: a worktree row (branch label, Git icon, current dot) and the
  repository chip the header, Changes and branch dialogs already draw, in the explorer's own header; listed as undrawn for the Designer.
- **Not done.** The delete confirm of a thread has no place for notes, so it does not say that a multi-repository thread's worktrees
  stay on disk when they hold changes (the service removes the clean ones; see "As built: pull requests and removal"). The optional first `message` is not asked for in the dialog (the thread opens and
  the person types). `ThreadRepo.pull_request`, `repo_id` on pull request routes and on `FileDiff` and removing a repository are now in the service ("As built: pull requests and removal"); the web side draws them (below).
- **Not verified against a live service.** Checked with the mock (which now applies the rules above) and request-level tests of the
  client; the routes have not been run against an `orbynd` with `ORBYN_MULTI_REPO=1`.

## As built: repo_id on Git and file calls

Behind the same flag. Migration 66 adds `thread_repos` (thread, repo id, position, name, branch, worktree path); a thread with no rows is
an ordinary thread and nothing below changes for it. The rows are written by thread creation in the next slice.

- **Where `repo_id` goes.** Body: `POST /api/threads/{id}/git/action`, `POST .../branches`, `POST .../branches/switch`, and
  `PUT .../files/content` (`GitAction`, `CreateBranch`, `SwitchBranch`, `WriteFile`). Query: `GET .../commits`, `GET .../diff`,
  `GET .../branches` and `GET .../files/content`. Absent is the primary repository (position 0).
- **One resolver.** Every thread Git and file call reads the thread through one function that, for a multi-repository thread,
  returns it with its `cwd` set to the chosen repository's worktree. So the repository is the only thing that changes: the path of a
  file is resolved inside that worktree by the usual guard (a path that leaves it, an absolute path or a link out is refused), and a
  file `revision` belongs to that repository's file. In Core the route sets the repository with `orbyn_core::in_repo(repo_id, work)`;
  work spawned from inside does not inherit it.
- **Refusals.** A `repo_id` on an ordinary thread, one that is not the thread's, or any `repo_id` with the flag off is `404 not_found`. A
  repository whose row in the project is untrusted is `409 repo_untrusted` for every call; a repository removed from the project keeps
  working in a thread that already has its worktree.
- **`GET /api/threads/{id}/repos`** answers `ThreadRepo[]` (`repo_id`, `name`, `branch`, `added`, `deleted`, `trusted`, `files` at most
  200 repo-relative paths): lines and files changed against `HEAD`, read with the hardened Git. `404` for an ordinary thread.
- **Pull requests per repository.** See "As built: pull requests and removal" below.

## As built: repository threads and the task folder

Behind the same flag; no new migration (`thread_repos` is from the slice above). Built: `POST /api/projects/{id}/repo-threads`,
`GET /api/threads/{id}/task-folder`, `POST /api/threads/{id}/repos`, and `repo_id` on the file tree and metadata reads.

- **Start.** `StartThreadInput { repos, engine, model, permission_mode, message?, branch_name? }`, answered with the `Thread`. Two to eight
  repositories (`409 repo_limit` past 8; fewer than two is `400`, one repository uses the normal start); each `base_branch` must be a
  local branch of that repository (`400`); a repository chosen twice is `400`. Every chosen repository, the project's own folder
  included, must be trusted, finished cloning and a Git repository: `409 repo_untrusted`, `400`, `409 not_git`. All of that is
  checked before anything is created. `permission_mode` must be one the engine offers (`400 permission_mode_unsupported`).
- **Branch.** One name for every repository: `branch_name` (a valid branch name, at most 200 characters), else `orbyn/<first 8 characters
  of the thread id>`. A repository that already has the name refuses the start (`400`), so nothing is overwritten.
- **Folder.** `<data>/threads/<thread id>/`, one worktree per repository at `<folder>/<name>` made with
  `git worktree add -b <branch> -- <path> refs/heads/<base>`. The folder is not a repository. The thread's `cwd` is the folder, so the
  engine's working directory and write scope contain every worktree and approvals name the absolute worktree path as always. If any
  worktree, the thread or its rows cannot be made, the worktrees and branches made so far, the folder and the thread are removed. The
  primary repository (the project's own folder, when chosen) is position 0; `permission_mode` is applied after creation and the optional
  `message` is queued as the first turn.
- **Reads.** `GET .../task-folder` answers `TaskFolder { root, branch, repos: ThreadRepo[], max_per_thread }`, `404` for an ordinary
  thread. `GET .../files` and `.../files/metadata` take `repo_id` too, so the tree of one repository can be shown.
- **Adding a repository** (`POST /api/threads/{id}/repos` `{ repo_id }`, `ThreadRepo`): a trusted repository of the project that is not
  in the thread, on the thread's branch cut from that repository's current branch (`400` when none is checked out, when it already has
  the branch name or is already in the thread; `409 repo_limit` at 8, `409 repo_untrusted`). Both writes are Person-class.
- **Not built.** Per-repository settings.

## As built: pull requests and removal

- **Links are per repository.** A pull request link is stored by (thread, repository, number) (migration 70). Every link made before it moved
  to the thread's own project, which is its primary repository (a test keeps old links alive). A repository id is the project id for the
  primary and the repository row id for the others, as everywhere else. `GET /api/projects/{id}/pulls/{number}/linked-threads` matches
  the project's own repository only, so another repository's #12 never lists a thread.
- **`repo_id` on the thread pull request routes.** `?repo_id=` on `GET` and `POST /api/threads/{id}/pulls`, `POST .../pulls/refresh`,
  `GET` and `DELETE .../pulls/{number}`, `POST .../pulls/{number}/review` and `.../merge`. Absent means the primary repository, so every
  older client is unchanged. Unknown, untrusted or flag-off repositories are refused as for the Git routes. Watches
  (`.../pulls/{number}/watch`) stay on the primary repository for now; unlinking a primary link drops its watch, any other link does not.
  The watch pass refreshes the open links of each repository of a thread.
- **`ThreadRepo.pull_request`.** The repository's open pull request with the highest number, else the latest one it has; absent when none.
- **`FileDiff.repo_id`.** Set on a thread diff of a multi-repository thread: the repository asked for, else the primary. Absent for an
  ordinary thread.
- **Deleting a multi-repository thread** removes each worktree that holds nothing to lose: a change, an untracked file or an ignored file
  keeps it, and so does a worktree outside the thread's own folder. Branches are kept, as for a single-repository thread. The folder is
  removed when empty. What was kept is in the `repo.thread.delete` audit row.
- **`DELETE /api/threads/{id}/repos/{repo_id}`** (Person-class, `204`) removes a repository from a multi-repository thread: the same worktree
  rule (`400` with "Commit or discard its changes first" when it holds something), no run in flight, never the primary repository.
  Its pull request links go with it; the branch is kept. `404` for an unknown repository.

Web side of pull requests and removal (mock-tested, not run against a live `orbynd`):

- **Repository menu.** Shows the repository's own pull request from `ThreadRepo.pull_request`. While it is open the menu offers Review and Merge
  (`threadReview`, `threadMerge`, `?repo_id=` left out for the primary) in place of Create pull request; a merged or closed one is named and
  Create pull request returns. The merge here sends no `remove_agent_credits`, so credits follow the account setting.
- **Remove from thread.** A button on each non-primary row of the Repos popover, behind a confirm dialog; the service's `400` message is shown as written.
- **Diff.** The service answers one repository per read, so the Changes panel still reads once per trusted repository; `FileDiff.repo_id` is not needed to group.
- **Merge dialog.** The "Remove agent credits" switch is the person's choice for that merge and is sent as `remove_agent_credits` (the account setting when untouched).
- **Not done.** A pull request screen per repository (the pulls page is keyed by project, not repository) and watches for a non-primary repository.

## Not in the beta

Per-repository settings, a thread that spans repositories of more than one project, committing across
repositories at once, and moving a thread between repositories. Each is a later row.

## Differences from the web side's guessed contract

- `RepoRow` adds `primary`.
- `create` (a new folder with `git init`) and `clone/cancel` are new calls; `startThread` is the route
  `repo-threads`, and `StartThreadInput` adds an optional `message`.
- `trust` on a repository is the only way an untrusted one becomes usable; `add-folder` does not trust.
- `threadRepos` answering `null` is a `404` from the route.
- The Changes list and Git actions gain `repo_id`.
- Web side, adopted as the document defines it: `publish` takes `name` and `visibility` only and answers the `RepoRow`; the `provider`, `remote`, `protocol` and `pushed` the web side had guessed are dropped. The Git actions in ONE repository (commit, push, pull request, merge, review) send the routes in "Per-repository Git routes" with `repo_id`. Not defined here, so the web client has no call and shows no control for them: a git pull, removing a repository from a thread (not in the beta) and `ThreadRepo.pull_request` (shown only when the service sends it). No call spans repositories.

## Tests

A clone source of `-x`, `ext::sh`, `file:///etc`, `fd::1`, a path, or with a newline is refused and starts no
process; a source is passed after `--`; a destination never leaves `clone_directory` through a name with
`..`, a separator or a symlink; an untrusted repository cannot be added to a thread or started in; a thread
over the limit is refused; each write route needs the person key; a failed clone is listed and removed on
`remove`; per-repository commit touches only that repository's worktree; a permission mode the engine
does not offer is refused before anything is created; a scan never follows a symlink, caps its results
and adds nothing; a workspace folder outside the file's folder is listed `refused` and not added; a
`repo_id` on a file write never resolves a path in another repository; a task folder uses one branch name in
every repository and a taken name creates nothing.
