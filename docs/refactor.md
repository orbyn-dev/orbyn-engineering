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
- One-way dependencies: `app` -> `features` -> `ui`, `lib`, `state` -> `api`.
  Shared pieces move to a shared place (`features/content` for markdown and code
  rendering, or `ui`).
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
| Web import direction, feature front doors, import cycles, file size (600 lines TypeScript, 1500 CSS) | `npm run check` in `web` (`scripts/arch-check.mjs`) | `web/scripts/arch-baseline.json` (direction and file size only; front doors, file cycles and feature cycles are always errors) |
| Rust: no new `.rs` file over 1500 lines; `orbynd` gains no `Command::new("git")` and no new core `store` references | `python3 scripts/check_rust_arch.py` (CI "Cheap checks") | `scripts/rust-arch-baseline.json` |
| Unused exports (report only, not in `check`) | `npm run arch:report` in `web` (knip) | none |

A unit that fixes violations or moves a baselined file runs `npm run arch:update`
or `python3 scripts/check_rust_arch.py --update` in the same PR. A baselined file
that grows fails (web and Rust); one that shrinks but stays long only prints a
notice, and `--update` lowers its recorded size.

Notes:

- The web layering is one way: `app` -> `features` -> `ui`, `lib`, `state` -> `api`.
  So features never import `app`, shared folders never import `app` or a feature,
  and `api` never imports `ui`, `lib` or `state`. These rules live here;
  [architecture.md](architecture.md) has no layering section.
- Features form one acyclic order. A feature imports only the `index.ts` of one lower
  down: `settings` > `onboarding` > `thread` > `panels` > `workspace` > `skills`, with
  `pulls` and `tools` below `thread` and `panels`, and `content` at the bottom.
  `usage` and `platform` import no feature. A piece two
  features need moves to the lowest layer that fits: `features/content` (markdown and
  code rendering), `ui`, `lib` (for example `lib/timeline`, the thread projection) or
  `state` (for example `composerChipStore`, `threadLogs`), not a re-export from the old place.
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
- `web/src`: `app/`, `api/`, `state/`, `lib/`, `ui/` (each control in its own folder
  with its CSS), `styles/` (tokens, base, layouts only), `generated/`, and
  `features/`: `thread/` (header, composer, timeline, trail, messages, approvals,
  details, git, mcp-apps), `content/` (markdown, code), `panels/` (host, strip,
  browser, diff, files, terminal, activity, side-chat, preview), `workspace/`
  (sidebar, palette, app chrome), `settings/` (one folder per section), `pulls/`
  (list, detail, model), `onboarding/`, `usage/`, `skills/`, `platform/`.

## Units

| Unit | Work |
| --- | --- |
| R0 | Guardrail checks (this page) |
| B1 | Split the contracts crate by area |
| B2 | `orbynd` routes by area (done: `routes/`, `http/`, `cli/`) |
| B3 | Big core files into folders |
| B4 | One process-spawn path; narrower visibility |
| W1 | Dead code and shared helpers |
| W2 | CSS ownership: no class defined twice |
| W3 | Break feature cycles; `features/content` and `index.ts` front doors |
| W4 | Per-feature `api.ts` and hooks |
| W5 | Giant components into sub-folders |
| W6 | Naming pass and `web/README.md` |

The backend (B) and web (W) tracks can run in parallel.

### Status

- R0, W1: done.
- W5 (in progress): `features/thread/composer/` holds the composer (`Composer.tsx` 1,109 to 597
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
  front door. The three long files left the baseline (10 to 7 long TypeScript files). The rest of W5
  (PullDetail and others) is still open.
- W3: done. `features/content/{markdown,code}` holds Markdown, its links, media, Mermaid,
  Tex, the pull-link hover card, code blocks, highlighting and ANSI. Moved down to
  shared layers: `state/{composerChipStore,threadLogs}`, `lib/{composerContext,
  useResolvedThreadPath,editorPreferences,runProjectAction,actionPreview,media}`,
  `lib/timeline/{projection,types,timeline,toolTrace,mcpAppBridge}`,
  `ui/{DetailRows,FolderBrowser}`. `pullStore` moved to `pulls`; `SideChatPane` and
  `sideChat` moved to `thread`, which hands the tab's content to `SurfacePane`; the git
  dialog and the restore-files dialog moved to `panels`. Front doors exist for
  `content`, `onboarding`, `panels`, `pulls`, `skills`, `thread`, `tools` and `workspace`.
  Deep imports 69 to 0 and cycle edges 8 to 0; both rules are now hard errors. Still
  baselined: 21 `direction` entries (features importing `app/shortcuts`,
  `app/commandLabels`, `app/sidebarDrawer`), the long files and `styles/thread.css`.
  Left for later units: CSS stays where it was (W2), giant components stay whole (W5).

## Done when

- No hand-written source file over about 800 lines.
- No web feature import cycles; the check fails if one returns.
- Global stylesheets hold only tokens, base and layout, with no class defined twice.
- `orbynd` holds routing and transport only, with one process-spawn path.
- Full checks pass, and so does a packaged-app check.
