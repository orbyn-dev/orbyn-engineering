# web

The React UI, shared by the desktop app and the browser. It holds task flows
only: authority and lifecycle rules live in the service
([docs/architecture.md](../docs/architecture.md)).

## Contract

It talks to `orbynd` only through the HTTP API and streams in `src/api`, using
the types generated from `crates/orbyn-contracts` into `src/generated`. Run
`npm run generate` after a contract change. It imports no Tauri API; ESLint
enforces that.

## Consumers

- `orbynd` serves the built `dist/` as its static UI.
- `desktop/src-tauri` bundles the same build.

## Folders

| Folder | Owns |
| --- | --- |
| `src/app` | The shell, title bar and startup; only the shell uses what lives here |
| `src/features/<name>` | One capability each: content, onboarding, panels, platform, pulls, settings, skills, thread, tools, usage, workspace |
| `src/api` | The HTTP client, streams, origin and token |
| `src/product` | Code that carries this product's vocabulary and two or more features use: `state/` (route, UI, workspace stores), `timeline/`, `projects/`, `composer/`, `engines/`, `git/`, `pulls/`, `shell/` (shortcuts), `tasks/`, `media/`, `panels/` (panel motion), `runs/` (state labels), `ui/` (product components) |
| `src/ui` | Generic visual primitives with no product words |
| `src/lib` | Generic helpers that would work in any repository |
| `src/styles` | Tokens, themes and page styles |
| `src/generated` | Generated contract types; never edited by hand |

Imports point inward: `app` -> `features` -> `product` -> generic (`ui`, `lib`,
`api`). Features may also use `styles` and `generated`. `product` and the generic
folders never import a feature or `app`, generic code never imports `product`, and
ESLint and `npm run check` fail the change that tries. A file used by one feature
lives in that feature; shared code appears only by hoisting (docs/refactor.md).
Each feature's `index.ts` exports API views, dialogs, commands and state only; pure
helpers live in `src/product` or `src/lib`.
Features import another feature only through its `index.ts`, in an acyclic order.
`npm run check` runs `scripts/arch-check.mjs`, which rejects deep cross-feature
imports, file import cycles and feature cycles without a baseline exemption
([rules and feature order](../docs/refactor.md#checks)).

## Names

Page is a route, Panel is a side-panel tab, Dialog is a modal. Non-route
components are `...View` (`TaskView`, `StartView`); only side-panel tabs are
`...Panel` (`PanelHost`, `SideChatPanel`, `BrowserPanel`); non-modal content is
not a `...Dialog` (`QuestionCard`, `ConfirmFacts`). There is no "Pane".

## Tests

Unit tests sit beside their file as `*.test.ts(x)` and run with `npm test`.
Browser tests are in `e2e/`, against the mock API (`mock/`) or a real service
(`e2e/real`).

## Known limitations

- The architecture checker matches text, not a TypeScript syntax tree. It follows
  relative string-literal imports and re-exports, including type-only imports,
  literal `import()` and import type queries. Computed specifiers and path aliases
  are not checked; unresolved relative imports only warn.
- Generated code, `*.test.ts(x)`, `test-setup.ts` and `test-support.ts` are excluded
  from the architecture graph. ESLint's `architecture/boundaries` rule covers static
  imports, re-exports, literal `import()` and type queries: another feature only
  through its `index.ts`, and shared folders never import a feature. A feature may
  use its own internals; tests are exempt. Computed specifiers are not checked.
