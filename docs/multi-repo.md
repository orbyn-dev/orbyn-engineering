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
  or push never spans repositories.
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

## Errors

The usual `{ code, message }`. `repo_untrusted` (409), `repo_exists` (409), `repo_limit` (409: more than
`max_per_thread`), `clone_refused` (400), `not_git` (409: an action that needs Git on a folder that has
none), `permission_mode_unsupported` (400), `not_found` (404), `invalid` (400).

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

## Tests

A clone source of `-x`, `ext::sh`, `file:///etc`, `fd::1`, a path, or with a newline is refused and starts no
process; a source is passed after `--`; a destination never leaves `clone_directory` through a name with
`..`, a separator or a symlink; an untrusted repository cannot be added to a thread or started in; a thread
over the limit is refused; each write route needs the person key; a failed clone is listed and removed on
`remove`; per-repository commit touches only that repository's worktree; a permission mode the engine
does not offer is refused before anything is created.
