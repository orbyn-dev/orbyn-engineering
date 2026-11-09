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
| `src/features/<name>` | One capability each: onboarding, panels, pulls, settings, thread, usage, workspace |
| `src/api` | The HTTP client, streams, origin and token |
| `src/product` | Code that carries this product's vocabulary and two or more features use: `state/` (route, UI, workspace stores), `timeline/`, `projects/`, `composer/`, `shell/` (shortcuts), `ui/` (product components) |
| `src/ui` | Generic visual primitives with no product words |
| `src/lib` | Generic helpers that would work in any repository |
| `src/styles` | Tokens, themes and page styles |
| `src/generated` | Generated contract types; never edited by hand |

Imports point inward: `app` -> `features` -> `product` -> generic (`ui`, `lib`,
`api`). Features may also use `styles` and `generated`. `product` and the generic
folders never import a feature or `app`, generic code never imports `product`, and
ESLint and `npm run check` fail the change that tries. A file used by one feature
lives in that feature; shared code appears only by hoisting (docs/refactor.md).

## Tests

Unit tests sit beside their file as `*.test.ts(x)` and run with `npm test`.
Browser tests are in `e2e/`, against the mock API (`mock/`) or a real service
(`e2e/real`).

## Known limitations

- Features still import each other, most often `panels` and `thread`. Each such
  import is a candidate for moving the shared piece to its real owner.
- Front doors still export some helpers (docs/refactor.md lists them).
- ESLint enforces the inward direction for shared folders only, and only for
  static `import` and `export … from`. Dynamic `import()`, type queries, imports
  between features and tests are not checked.
