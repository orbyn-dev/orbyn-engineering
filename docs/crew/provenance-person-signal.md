# Who wrote a line: a positive person signal for merge provenance (4l)

Design note for issue #711. Status: proposal, nothing built. Until it is built every line stays `agent` and coverage says merge-level only.

## The problem

Merge provenance (`crew/capacity/provenance.rs`) labels each added line through `git blame` of the merge commit. A line is a person's only when its last-changing commit is in the positive `person_commits` list the caller passes; the merge hook passes an empty list. The list cannot be computed from authorship today:

- Workers run in the lead's checkout, as the person, and commit with the person's own git identity. An author match would label agent commits as the person's and overstate review coverage.
- No known person identity exists in config, and the person also commits by hand, in an editor or a terminal, where the app sees nothing.

A wrong "person" label is worse than a missing one: coverage would count an unreviewed agent line as written by a human. So the rule stays fail-closed: a commit is the person's only on positive evidence, and everything else counts as agent for coverage.

## Recommendation

Mark agent commits positively, mark person commits positively only where the app itself can see them, and keep a third value for the rest.

1. **A distinct worker identity.** Every process a Crew worker runs (the engine and its shell tools, through `host::child_env::for_tool`) gets `GIT_AUTHOR_NAME`, `GIT_AUTHOR_EMAIL`, `GIT_COMMITTER_NAME` and `GIT_COMMITTER_EMAIL` set to `Orbyn Crew worker (<engine>/<model>) <worker+<task id>@orbyn.invalid>`. The environment overrides the person's git config for that process only; the person's own config is not touched. The task id in the address ties a commit to its worker task.
2. **A trailer as a second mark, best effort.** The worker brief asks for `Orbyn-Task: <task id>` in commit messages. The app cannot enforce it (the person owns the repository's hooks), so the identity is the mark and the trailer only corroborates it.
3. **Person commits only for commits the app makes on a person action.** When a person commits through the app, or accepts a change the app then commits, the app records the full commit id in a local table (`crew_person_commits`: project, commit, time, action) in the same transaction as the audit row. `person_commits` is read from that table. A commit the person makes outside the app is not listed.
4. **Three values, two counts.** A line is `agent` (worker identity or task trailer matches), `person` (commit id is in the table) or `unknown` (anything else: the person's manual commits, history before this change, bots). Coverage counts `unknown` as agent: it is never credited as human-written. The screens show the three counts apart.
5. **A person may attest.** Later, a Person route can mark a merge "I wrote this" (audit first, `seen`). It adds those commit ids to the table. Nothing is inferred from an email match.

## What it does not do

- The identity is a label, not a boundary. A worker can run `git -c user.name=...` and commit as anyone. It is enough for coverage (a worker that hides itself shows up as `unknown`, which counts as agent) and is never used to authorise anything.
- It does not tell a person's manual commit from a bot's: both are `unknown`.

## What changes for existing setups

- **History.** Existing commits carry no worker identity and are not in the table: all `unknown`, so coverage for old merges does not change.
- **Existing checkouts.** Workers still run in the lead's checkout. The environment override works there without a worktree. When worker worktrees are built (planned), the same environment applies, and each worktree can also set the identity in its own `config.worktree`.
- **Engines.** Engines that pass their own `-c user.name` or add `Co-Authored-By` trailers keep doing so; their commits are `unknown` unless the environment wins. Each engine needs a probe: commit once in a scratch repository and read the author.
- **Sandbox.** The macOS worker profile does not touch environment variables or git. The escape-check proof environment is cleared to a short list and never commits, so it is unaffected. The proof's own commits do not exist.
- **The app's own commits** (merge commits, notes under `refs/notes/orbyn-agent-trace`, `Orbyn Crew <crew@orbyn.invalid>`) are not labelled as a person's. A merge commit adds no hunk of its own.
- **Cost.** One small table and a migration (next free number at merge), one environment block in the worker spawn, and a trailer line in the brief.

## Open questions for the owner

- Do we want the person's manual commits credited at all in v1, or is "unknown counts as agent" the right permanent default until a person attests?
- Is a visible `unknown` third count acceptable on the coverage screens, or should it fold into `agent` in the UI?
