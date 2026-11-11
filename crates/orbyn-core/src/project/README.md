# Project

Projects and the files inside them, and the one managed workspace that holds them.

| Module | Owns |
| --- | --- |
| `projects` | Registering, cloning and creating projects |
| `workspace` | The managed library checkout and its `repo/` targets |
| `project_file` | The team's read-only `orbyn.json` project file |
| `project_setup` | First-turn admission for a worktree's setup action |
| `project_scopes` | Settings targets derived from registered projects |
| `project_icons` | Passive image reads for registered projects |
| `project_icon_override` | Project display metadata validation |
| `project_image_import` | A project image chosen outside the project |
| `project_image_picker` | Searching a project for candidate images |
| `explorer` | A project's worktrees and a read-only, lazily read look at their files, with change letters from one kept `git status` per worktree |
| `files` | Reading and editing a thread's checkout inside its write scope |
| `file_bytes` | Raw bytes of a checkout file for media and binary views |
| `outside_files` | Read-only files outside a thread's checkout |

`workspace` owns the managed library and project boundary, including the lock
that coordinates library replacement with run admission. `conversation::runs`,
`agents::console`, and `operations::updates` use that boundary; their use does
not make them owners of the library lifecycle.

The crate's [group table](../../README.md#groups) lists every group.

## Explorer rules

`explorer` is a person's read-only view; nothing in it writes.

- A worktree is named by an id Core derives from its real path and finds only in the
  project's own `git worktree list`. A path from the client is never a worktree.
- A path inside it is relative, and refused by spelling (absolute, `..`, backslash, control
  characters, `.git` in any case, and on Windows drives, streams, short names and devices),
  then resolved with links followed and kept inside the worktree. Git's folder, Orbyn's data
  folder and the library's `repo/` are out of reach. A final link is never followed, and
  devices, pipes and sockets are neither listed nor opened.
- A folder read returns at most 1,000 entries and says `truncated`; a file read returns text
  up to 4 MiB, or a `binary` or `too_large` marker.
- Change letters, ignored paths and the dirty summary come from one `git status` per
  worktree through `source_control::git`, kept as the architecture's
  [Explorer](../../../../docs/architecture.md#explorer) section says.
