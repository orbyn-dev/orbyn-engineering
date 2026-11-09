# Folder-architecture refactor

Goal: a layout that is easier to navigate and maintain, with no behaviour
change. One unit is one PR. Moves (`git mv`) and edits go in separate commits,
and every move commit is added to `.git-blame-ignore-revs`. Generated TypeScript
type names and CSS class names stay the same unless a unit says otherwise. A rule
change in [architecture.md](architecture.md) goes in the same commit.

Run once per clone so blame skips the move commits:

```sh
git config blame.ignoreRevsFile .git-blame-ignore-revs
```

## Rules the layout keeps

- One folder per capability or feature; big topics get sub-folders.
- One front door per feature: `features/<x>/index.ts` is the only file another
  feature imports. No deep imports across features.
- One-way dependencies: `app` -> `features` -> `product` -> generic (`lib`, `ui`, `api`).
  Shared code has two tiers (see "Tiers for shared code" below); a piece two features
  need moves to the lowest tier that fits (`features/content` for markdown and code
  rendering, `product`, `ui` or `lib`).
- Styles, tests and API hooks sit beside the component they belong to. Global
  styles hold only tokens, base and layout.
- One name, one meaning: Page is a route, Panel is a side-panel tab, Dialog is a
  modal. No "Pane".
- Soft cap of about 600 lines per hand-written web file (generated code excepted):
  a bigger one becomes a folder.
- Rust: `orbynd` is transport only; one way to spawn git and processes; one JSON
  helper; one home-directory lookup.

## Checks

Existing violations of some rules are recorded in a baseline that may only shrink.
A new violation fails, and so does a baselined item that no longer exists, so a
fixed violation cannot quietly return: re-record the baseline in the same PR.
Deep cross-feature imports, import cycles and cycles between features have no baseline: one fails
at once.

| Check | Run | Baseline |
| --- | --- | --- |
| Web import direction, tiers, feature front doors, import cycles, file size (600 lines TypeScript, 1500 CSS) | `npm run check` in `web` (`scripts/arch-check.mjs`, plus the ESLint import rules) | `web/scripts/arch-baseline.json` (direction, tier and file size only; front doors, file cycles and feature cycles are always errors) |
| Rust: no new `.rs` file over 1500 lines; `orbynd` gains no `Command::new("git")` and no new core `store` references | `python3 scripts/check_rust_arch.py` (CI "Cheap checks") | `scripts/rust-arch-baseline.json` |
| Unused exports (report only, not in `check`) | `npm run arch:report` in `web` (knip) | none |

A unit that fixes violations or moves a baselined file runs `npm run arch:update`
or `python3 scripts/check_rust_arch.py --update` in the same PR. A baselined file
that grows fails (web and Rust); one that shrinks but stays long only prints a
notice, and `--update` lowers its recorded size.

Notes:

- The web layering is one way: `app` -> `features` -> `product` -> generic (`ui`, `lib`, `api`).
  So features never import `app`, `product` and the generic folders never import `app` or a
  feature, generic code never imports `product` (rule `tier`), and `api` never imports `ui` or
  `lib`. Both `direction` and `tier` have empty baselines. These rules live here;
  [architecture.md](architecture.md) has no layering section.
- Features form one acyclic order. A feature imports only the `index.ts` of one lower
  down: `settings` > `onboarding` > `thread` > `panels` > `workspace` > `skills`, with
  `pulls` and `tools` below `thread` and `panels`, and `content` at the bottom.
  `usage` and `platform` import no feature. A piece two
  features need moves to the lowest layer that fits: `features/content` (markdown and
  code rendering), `product` (for example `product/timeline`, the thread projection, or
  `product/state/threadLogs`), `ui` or `lib`, not a re-export from the old place.
- Type-only imports count as edges, like any import. Imports inside comments are
  ignored, and an unresolved relative import prints a warning.
- Both checks are text matching, not parsers; the Rust one lists its known gaps
  (for example `Command::new` with a const) in its docstring. CI runs them on Ubuntu
  only.

## Target tree

- `crates/orbyn-contracts/src`: `lib.rs` (re-exports only) plus projects, threads,
  files, git, pulls, engines, usage, settings, terminal, automations, governance.
- `crates/orbyn-core/src`: `app.rs`, `error.rs`, `util/`, `store/`, `testing.rs`,
  then the capability groups: `agents/` (driver per engine, engines, history, tools,
  usage, selection, titles, permission modes), `conversation/` (runs, threads,
  composer, questions, checkpoints, file changes, automations, renders, search,
  scratch), `source_control/` (`git.rs` is the one way to run git; forge, scm, jj,
  pull guides), `browser/`, `project/`, `governance/`, `tools/`, `settings/`,
  `operations/`, `host/`.
- `crates/orbynd/src`: `main.rs` (start-up only), `routes/` by area, `http/`
  (errors, json, access, remote images), `cli/`, `bin/`.
- `web/src`: `app/` (shell only), `api/`, `product/` (`state/`, `timeline/`, `projects/`,
  `composer/`, `shell/`, `tasks/`, `media/`, `panels/`, `runs/`, `ui/`), `lib/` and `ui/`
  (generic: each control with its CSS), `styles/` (tokens, base, layouts only), `generated/`, and
  `features/`: `thread/` (header, composer, timeline, trail, messages, approvals,
  details, git, mcp-apps), `content/` (markdown, code), `panels/` (host, strip,
  browser, diff, files, terminal, activity, side-chat, preview), `workspace/`
  (sidebar, palette, app chrome), `settings/` (one folder per section), `pulls/`
  (list, detail, model), `onboarding/`, `usage/`, `skills/`, `platform/`.

## Units

| Unit | Work | State |
| --- | --- | --- |
| R0 | Guardrail checks (this page) | done |
| B1 | Split the contracts crate by area | done |
| B2 | `orbynd` routes by area (`routes/`, `http/`, `cli/`) | done |
| B3a | Engine drivers into folders (`claude`, `codex`, `acp`) | done |
| B3b | Source control into folders (`forge/`, `scm/`) | done |
| B3c1 | Preview and MCP tools into folders | done |
| B3c2 | `conversation/runs.rs` (1,996 lines) and `agents/engines/mod.rs` (1,659) into folders | done |
| B4 | One process-spawn path; narrower visibility | open |
| W1 | Dead code and shared helpers | done |
| W2 | CSS ownership: no class defined twice | open, needs the owner's go-ahead |
| W3 | Break feature cycles; `features/content` and `index.ts` front doors | done |
| W4 | Per-feature `api.ts` and hooks | open (not started) |
| W5 | Giant components into sub-folders (composer, trail, sidebar, panels, settings, pulls, usage) | done |
| W6 | Naming pass and `web/README.md` | `web/README.md` exists (W7a); the naming pass is open |
| W7a | Product and generic tiers for shared web code (`product/`) | done |
| W7b | Split `app/Shell.tsx` (732 lines), `product/state/ui.ts` (783) and `onboarding/DraftView.tsx` (608) | done |

The backend (B) and web (W) tracks can run in parallel.

### Status

- R0, W1: done.
- W5: `features/thread/composer/` holds the composer (`Composer.tsx` 1,109 to 597
  lines, with the mention menu, constants, key and paste handling, attachment, stash, stop
  and thread-setting hooks, field, bar, context row and suggestion loading each in a file
  of their own) and `features/thread/trail/` holds the tool trail (`ToolTrail.tsx` 876 to
  261 lines, `PlanRow.tsx`, and `blocks/` with one renderer file per kind). Both left the
  long-files baseline (13 to 11 long TypeScript files). `features/workspace/` now has `sidebar/`
  (`Sidebar.tsx` 1,199 to 590 lines, with its list rows, chrome, drag-and-drop helpers, folder menu,
  hooks, search field and delete dialog in files of their own, plus thread rows, menu, actions, undo
  and grouping), `palette/` (command palette, add-project palette, project filter, path input) and
  `app-chrome/` (updates, quit confirmation, environment identity, notices, star prompt, clone jobs);
  `Sidebar.tsx` left the baseline (11 to 10 long TypeScript files). `features/panels/` now has
  `strip/` (tabs, width), `browser/` (`BrowserPanel.tsx` 954 to 440 lines, with the page view,
  resize handles, start page and the address, zoom, device and recent-page rules in files of their
  own), `diff/` (`DiffPanel.tsx` 665 to 377 lines, with the diff model and the patch view; the git
  action and restore-files dialogs), `files/` (viewers, zip and Office readers; `office.ts` 720 to
  457 lines), `terminal/`, `activity/` and `preview/`; `SurfacePane.tsx` keeps its name and sits at the
  front door. The three long files left the baseline (10 to 7 long TypeScript files).
  `features/settings/` has
  `rows/` (scoped preference rows, scope context and sentence, banner, dialog card), `sections/`
  (one file per settings section; `PreferenceSections.tsx` 796 lines is gone), `pages/`
  (Appearance, Keybindings, Threads), `providers/`, `projects/`, `maintenance/`, `tools-connectors/`
  and `scheduled-tasks/`; its CSS stays at the root until W2. `features/pulls/` has `list/`, `detail/`
  and `model/` (`PullDetail.tsx` 900 to 573 lines, `PullsToolbar.tsx` 676 to 289), and
  `features/usage/` keeps its files side by side (`UsagePage.tsx` 763 to 247, with the view, limits,
  parts and price dialog in files of their own). All four left the baseline (7 to 3 long TypeScript
  files). `features/tools` stays its own feature: `thread` and `panels` import it, and `settings`
  sits above them, so folding it into `settings/tools-connectors/` would be a cycle.
- W7b: done. `app/Shell.tsx` (732 to 150 lines) is `app/shell/`: `Shell.tsx` (the component),
  `routes.tsx` (home and the route switch), `commands.ts` (the commands the shell owns, focus
  cycling, jump rows), `keys.ts` (the global key listener, its context, the undo shortcut and the
  slow-request toast), `loading.ts` (the one-at-a-time workspace and engine loads), `notifications.ts`
  (the announcer, the chime and thread notices), `focus.ts` (route focus and toast clearance),
  `events.ts` (desktop shell events) and `index.ts` as the front door. `product/state/ui.ts` (783)
  is `product/state/ui/`: `types.ts` (the vocabulary and shapes, `Prefs` included), `defaults.ts`
  (first-run values and the layout helpers), `savedPrefs.ts` (reading, migrating and cleaning what
  was saved), `store.ts` (the store and its actions), `theme.ts` (`resolveTheme`, `applyTheme`) and
  `index.ts`, which exports exactly what the single file did, so about 60 importers are untouched.
  `features/onboarding/DraftView.tsx` (608 to 532) is `DraftView/`: `index.tsx` (the component),
  `DraftModelControls.tsx`, `suggestions.ts` and `draftHelpers.ts`. The web long-file baseline is
  empty of TypeScript (3 to 0); only `styles/thread.css` stays, for W2.
- W3: done. `features/content/{markdown,code}` holds Markdown, its links, media, Mermaid,
  Tex, the pull-link hover card, code blocks, highlighting and ANSI. Moved down to
  shared layers: `state/{composerChipStore,threadLogs}`, `lib/{composerContext,
  useResolvedThreadPath,editorPreferences,runProjectAction,actionPreview,media}`,
  `lib/timeline/{projection,types,timeline,toolTrace,mcpAppBridge}`,
  `ui/{DetailRows,FolderBrowser}`. `pullStore` moved to `pulls`; `SideChatPane` and
  `sideChat` moved to `thread`, which hands the tab's content to `SurfacePane`; the git
  dialog and the restore-files dialog moved to `panels`. Front doors exist for
  `content`, `onboarding`, `panels`, `pulls`, `skills`, `thread`, `tools` and `workspace`.
  Deep imports 69 to 0 and cycle edges 8 to 0; both rules are now hard errors. The 21
  `direction` entries it left baselined (features importing `app/shortcuts`,
  `app/commandLabels`, `app/sidebarDrawer`) were removed by W7a. CSS stays where it was (W2).

- B3a (engine drivers): done. `agents/driver/{claude,codex,acp}` are folders with the
  driver in `mod.rs` and its pieces beside it (`claude`: args, settings, mcp, catalog,
  protocol, events, plan; `codex`: protocol, sandbox, turn, events, catalog, tools, limits,
  short_sessions; `acp`: sessions, events, models, permissions, replay, sign_in), tests in
  `tests.rs`. `claude.rs` (3,438 lines), `codex.rs` (2,726) and `acp.rs` (1,507) left the
  Rust size baseline; the largest driver file is now 642 lines (`claude/events.rs`). Public
  paths such as `agents::driver::claude::run_args` are unchanged.

- B3b (source control): done. `source_control/forge/` holds `Forge` and the public functions
  in `mod.rs` (130 lines) with `process` (one CLI call), `parse`, `github`, `gitlab`,
  `details`, `pulls`, `review`, `merge`, `create`, `status`, `cache` (was `forge_cache.rs`)
  and tests in `tests.rs`; `source_control/scm/` holds the thread actions in `mod.rs` with
  `pulls`, `review`, `stack`, `watch`, `forges` and `writing/` (`candidate`, `style`;
  was `scm_writing`). `forge.rs` (2,987 lines) left the Rust size baseline; the largest
  non-test file is now 472 lines (`scm/writing/mod.rs`). `git.rs` stays whole (609 lines
  without tests). Public paths such as `source_control::forge::list_pulls` are unchanged.

- B3c (preview and MCP tools): done. `browser/preview/` holds the manager in `mod.rs` (207 lines)
  with `open`, `tabs`, `pump`, `crash`, `snapshots`, `expressions`, `profiles`, `servers` (local
  server discovery), `favicon` and tests in `tests.rs`; the former `preview_bridge`, `preview_recording`
  and `preview_trace` are `preview::{bridge, recording, trace}`. `tools/mcp/` holds the server in `mod.rs`
  with `apps` (was `mcp_apps`) and `tools/` (was `mcp_tools`: `definitions`, `dispatch`, `guide`,
  `project`, `preview`, `render`, tests in `tests.rs`). `preview.rs` (2,507 lines) and `mcp_tools.rs`
  (1,920) left the Rust size baseline (5 to 3 long files); the largest non-test file is now
  `preview/recording.rs` (934, moved whole) and the largest cut file `tools/dispatch.rs` (682, one
  match). The old paths `browser::preview_bridge` and `tools::mcp_tools` still resolve.

- B3c2 (run manager and engine manager): done. `conversation/runs/` holds `RunManager` in `mod.rs`
  (372 lines: the structs, constants, `new` and the shared helpers) with `admission` (admission,
  launch, titles, the queue, steering, workflow stops and app notes), `stops` (cancel, forced stop,
  shutdown and the parent report), `execute` (one turn's execution, the next turn's warm process and
  limit stops), `checkpoints` (start and end snapshots, close, drive and session recovery), `turn`
  (attempt, start, model fallback, the event pump, event writes and approvals), plus `shape.rs` and
  tests. `agents/engines/mod.rs` keeps the registry and manager (1,113 lines) and its inline tests
  moved whole to `tests.rs` (539). `runs.rs` (1,996) and `engines/mod.rs` (1,659) left the Rust size
  baseline (3 to 1 long files). Methods a sibling module calls are `pub(super)`; no public path
  changed, and a model pool's item list and the 839-test list are identical before and after.

- W7a (product and generic tiers): done for the shared folders; front-door helpers are open (see
  the audit below). `state/` became `product/state/`, and `lib/timeline` became `product/timeline`.
  The shared files that carry product vocabulary and have two or more feature users moved to
  `product/` (`git mv`, then an import commit). Files with one feature user moved into it
  (`SettingRow`, `commandLabels`, `SlidersIcon`, `pageLabel`). The 21 `direction` entries
  (features importing `app/`) are gone: `app/shortcuts` and `app/sidebarDrawer` are
  `product/shell/`. New rule `tier` (generic code never imports `product/`), enforced by
  `arch-check` and by ESLint. Non-move edits: `useLingering` and `useSettledAfter` moved verbatim
  from `panelAnimations` into `lib/lingering.ts` (so `ui/Presence` stays generic), and `runStateLabels`
  and `authLabels` moved verbatim out of `lib/format.ts` into `product/runs/stateLabels.ts`.

### Tiers for shared code

Two tiers, one owner per file. `product/` holds code whose names or types carry this
product's vocabulary (threads, projects, runs, models, pulls, panels, composer) and that two or
more features use. Generic code (`lib/`, `ui/`, `api/`) would work in any repository. Code only
one feature uses stays in that feature. Shared tiers grow only by hoisting, and breaking a cycle
may hoist at the second caller. The old tier names `lib` and `ui` keep their names.

Classification of the former shared folders (features = how many features import it):

| Old home | Files | Tier | Where now |
| --- | --- | --- | --- |
| `state/*` stores (`ui`, `route`, `workspace`, `connection`, `draftStore`, `modelStore`, `composerChipStore`, `threadLogs`, `healthToasts`) | 9 | product (app state; 3 to 10 features) | `product/state/` |
| `state/pageLabel` | 1 | local (`pulls` only) | `features/pulls/detail/` |
| `lib/timeline/*` | 5 | product (thread projection; `thread`, `panels`) | `product/timeline/` |
| `lib/{projectIdentity,projectImages,projectImagePersistence,projectImagePreview}` | 4 | product (3 features, `ui`, `state`) | `product/projects/` |
| `lib/{composerContext,skillMention,actionCommand,runProjectAction,actionPreview,editorPreferences,useResolvedThreadPath}` | 7 | product (2 to 3 features) | `product/composer/` |
| `lib/taskSchedule` | 1 | product (`settings`, `thread`) | `product/tasks/` |
| `lib/media` (`rawUrl(threadId, ...)`) | 1 | product (`content`, `panels`) | `product/media/` |
| `lib/panelAnimations` (`usePanelMotion`, reads the UI store) | 1 | product (`settings`, `thread`, `app`) | `product/panels/` |
| `lib/format`: `runStateLabels`, `authLabels` | 2 consts | product (5 features) | `product/runs/stateLabels.ts` |
| `app/shortcuts`, `app/sidebarDrawer` | 2 | product (6 features and `app`; 1 feature and `app`) | `product/shell/` |
| `app/commandLabels` | 1 | local (`settings` only) | `features/settings/pages/` |
| `app/{barDrag,environmentTheme,navHistory,pollSchedule,readingNav,taskNotifications}`, `App`, `Shell`, `TitleBar` | 9 | app only | stay in `app/` |
| `ui/{DetailRows,FolderBrowser,Logo,ProjectActionIcon,ProjectIconVisual,PageBar,Toasts,badges,states}` (and their CSS and tests) | 9 | product (name or import carries a product word: thread details, project, route, UI store; 2 to 11 features) | `product/ui/` |
| `ui/SettingRow`, `ui/SlidersIcon` | 2 | local (`settings`; `workspace`) | into those features |
| `ui/{Collapsible,Dialog,Menu,Presence,Segmented,controls,focusReturn,openState,tabKeys,useEscapeClose,usePopover}` | 11 | generic | stay |
| `lib/{format,storage,motion,useAsync,useLayout,useNow,keptData,clearSelection,inputModality}` | 9 | generic | stay (`lib/lingering` is new: the two verbatim hooks above) |

Counts of hand-written files (no tests): `product/` 43 (40 TypeScript, 3 CSS), `lib/` 10 (was 28),
`ui/` 11 (was 25), `app/` 9 (was 12), `state/` 0 (was 10). `ui/openState` keeps thread words in comments
only, and its names are generic, so it stays generic.

Classification TODOs (not moved now): `product/state/connection.ts` is a generic offline store
and could move to `lib/`; `lib/useLayout.ts` names `panelOverlays`, so it may be product; `ui/openState.ts`
mentions threads in comments only.

### Front-door audit

Every export of a `features/<x>/index.ts` another feature imports. API is a view, dialog, command
or state other features use. A helper is a builder, formatter or pure function; one used by a single
other feature is a TODO row, hoisted to `product/` or `lib/` when a second feature needs it or it
breaks a cycle. The check does not yet limit what a front door may export (skipped: it needs a
curated list of API names; the table is the record).

| Feature | API | Helpers (TODO) |
| --- | --- | --- |
| `content` | `Markdown`, `CodeBlock`, `Mermaid`, `Lightbox` | `fileLinkTarget`, `markdownText`, `outputSpans` (all `thread` only) |
| `onboarding` | none | `chooseDraftEngine` (`settings` only) |
| `panels` | `GitActionDialog`, `PublishDialog`, `RestoreFilesDialog`, `SurfacePane`, `TerminalBar`, `TerminalDrawerBody`, `useGitAvailable` | `PLANS`, `Plan`, `createPullRequestPlan` (`thread` only) |
| `pulls` | `ToolbarPopover`, `useThreadPulls` | `WATCH_TIP`, `checksSummary`, `chipDot`, `chipLabel`, `entryOf`, `forgeName`, `numberLabel`, `readyToMerge`, `stateWord` (`thread` only) |
| `skills` | `SkillsNav` | none |
| `thread` | `ModelPickerTrigger`, `EffortTrigger`, `useTraitsMenu` | `ACCESS`, `offeredModes`, `shownAccess` (`onboarding` only), `DETAIL_ROWS`, `detailsMenuItems` (`settings` only) |
| `tools` | `ToolsList`, `ToolsRow`, `ToolsTab` | none |
| `workspace` | `AddProjectPalette`, `GitHubStarPrompt`, `RenameThreadDialog`, `recordUserPrompt`, `archiveThreads`, `deleteThreadNow`, `settleThreads`, `unarchiveThread`, `unsettleThreads`, `wakeThreads`, `snoozeChildren`, `threadRunState` | `copyText` (`thread` only; generic, wants `lib/`) |

## Handoff

The refactor stops here for now. Whoever continues starts from this section. Current numbers:
the web architecture baseline has no long TypeScript files and 1 long CSS file (`styles/thread.css`,
which is W2); the Rust baseline
has 1 long file (`bin/orbyn_fake_engine.rs`, a test double). The
`direction` and `tier` web baselines are empty, and deep imports, file cycles and feature cycles
have no baseline.

### What is open, in the order to do it

| Unit | Work | Size and notes |
| --- | --- | --- |
| B4 | One process-spawn path (git always through the wrapper; today about 8 direct `Command::new("git")` sites in core), one JSON helper (three different `field()` helpers), one home-directory lookup (about five private copies), and visibility narrowing checked with the compiler (the scout's name-matching list is not enough) | Rust. Changes behaviour paths, so each change needs its own test. |
| W2 | CSS ownership: `styles/thread.css` (about 4,500 lines, mostly panels and onboarding rules), `pages.css`, `shell.css`, `base.css` move next to the components they style; shared pieces (`btn`, `dialog`, `pill`, `empty` and the like, defined in up to 10 files) are defined once in `ui/` | **Needs the owner's go-ahead.** The tests cannot see a changed cascade. Compare screenshots before and after in Light, Dark and Dim for every moved rule, and keep the CSS load order identical (compare the first-import order from `main.tsx`, as the W7a review did). |
| W4 | Per-feature `api.ts` and hooks: components stop calling the API client directly (about 150 direct imports, for example Maintenance, BrowserPanel, the terminal drawer, the settings pages) | Web. Not started. |
| W6 | Naming pass: Page is a route, Panel is a side-panel tab, Dialog is a modal, no "Pane" (`SurfacePane`, `SideChatPane`, `PullsPage` against `PullPage`); keep `web/README.md` in step | Do after W2 so files are renamed once. |
| Front-door TODO rows | Hoist the helper exports in the audit table (one other feature uses each) to `product/` or `lib/` when a second feature needs them; hoist `copyText` to `lib/` | Opportunistic. |
| Closeout | Turn any remaining baselined rule into a hard error, run a packaged-app build and walk-through (`cd desktop && npm run build -- --bundles app`), run the worktree cleanup report | The packaged-app check has not been run since the logo, tools, picker and tab strip merged into main. |

### How a unit is run

1. One worktree per unit, created by the lead from `origin/main`; a worker never creates its own.
   Reuse the worktree of a merged unit (and its `target/` or `node_modules`) instead of making a new one.
2. First pin what must not change: a route table or string-literal set for Rust, a characterization
   test where the code has no coverage, and `rg` the old path across the repo (`web/`, `docs/`,
   `scripts/`, configs) before moving anything: a Rust-only PR skips the Web CI job, so a web test that
   reads a Rust file by path fails only on `main`.
3. Move with `git mv` in one commit (renames only), fix imports in the next, then cut pieces out in
   verbatim commits. List in `.git-blame-ignore-revs` only commits that are renames with zero edits,
   verified with `git diff -M --stat`, in the `<40-hex> # subject` format. A split into new files also
   needs `git blame -C -C -C`.
4. Prove neutrality: Rust item names and the test list (`cargo test -p orbyn-core -- --list`) before and
   after, plus the multiset of string literals; web test counts and, for a move, the first-import order
   of CSS and stores. Extract loading and error JSX as plain functions, not new components, or React
   remounts the page and drops focus (a failing test first).
5. An Opus review reads the PR before merge. Re-run `npm run arch:update` or
   `python3 scripts/check_rust_arch.py --update` in the same PR when a baselined file moves or shrinks.
6. Merge main into the branch right before each push, and queue PRs that edit the shared registry files
   (`.git-blame-ignore-revs`, this page, the arch baselines) one at a time: the first merge makes the next
   conflict. Merge by merge commit with every check green on the current head.

### Open items outside the units

- Issue #381: harden request values that reach git, `gh`, `glab`, the OTLP check and batch sizes. Five
  CodeQL alerts (10, 13 to 16) were dismissed as won't-fix with that issue as the reason, after moving
  handlers re-fingerprinted them as new on a PR. Close them with fixes, not dismissals.
- Issue #390: three flaky real specs (two preview-browser crashes, one pulls reload wait). A re-run is
  fine for a named infrastructure failure only (the NSIS download 504 in the Windows desktop build, `gh`
  network timeouts), after the run finishes and with the cause written in the PR. Caching the NSIS tool in
  the desktop build workflow would remove the most frequent one.
- Disk: each Rust worktree builds its own `target/` (several GB each) and the cleanup tool only moves
  retired worktrees to the Trash, so the space comes back when the Trash is emptied. Run
  `worktree_retire.py report` before creating a worktree.

## Done when

- No hand-written source file over about 800 lines.
- No web feature import cycles; the check fails if one returns.
- Global stylesheets hold only tokens, base and layout, with no class defined twice.
- `orbynd` holds routing and transport only, with one process-spawn path.
- Full checks pass, and so does a packaged-app check.
