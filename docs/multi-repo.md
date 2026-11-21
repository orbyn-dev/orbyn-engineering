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
`add-folder`, `create`, `clone`, `cancel`, `trust`, `setup-git`, `remove` and the folder scan with `.code-workspace` import. **Not built
yet** (the next slice): `repo-threads`, the thread's repository routes, the task folder, `repo_id` on the Git and file routes, and `publish`
(it checks the repository is trusted and then answers `422 unsupported`: "Publishing a repository arrives in a later update.").

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
- Still hand-written in `contract.ts` until their routes ship: `StartThreadInput`, `ThreadRepo`, `TaskFolder`, the thread Git actions.
  The mock (`web/mock/repos.ts`, hidden behind the preview flag) follows the generated shapes and the service's answers above.

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
