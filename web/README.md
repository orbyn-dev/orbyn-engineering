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
| `src/app` | The shell, shortcuts and startup |
| `src/features/<name>` | One capability each: onboarding, panels, pulls, settings, thread, usage, workspace |
| `src/api` | The HTTP client, streams, origin and token |
| `src/state` | App-wide stores: route, UI, workspace, health toasts |
| `src/ui` | Visual primitives with no business rules |
| `src/lib` | Small helpers shared by several features |
| `src/styles` | Tokens, themes and page styles |
| `src/generated` | Generated contract types; never edited by hand |

Imports point inward: `app` may use features, and features may use the shared
folders (`api`, `state`, `ui`, `lib`, `styles`, `generated`). Shared folders never
import a feature or `app`, and ESLint fails the change that tries. A file used by
one feature lives in that feature.

The `usage` feature exposes `UsagePage` through `features/usage/public.ts`.
The `platform` feature exposes `PreviewScreen` and `previewScreenRoute` through
`features/platform/public.ts`, preserving dev-only lazy preview loading.
All production consumers outside these features, including `app`, use their
public entry points. ESLint rejects private imports, re-exports, literal dynamic
imports and type queries into them; each feature may use its own internals.
Other features will adopt the same boundary one at a time. This follows Luan
Nguyen's public-entry-point approach in [PR #285](https://github.com/orbyn-dev/orbyn-engineering/pull/285);
the usage and platform facades are independently designed from current consumers.

## Tests

Unit tests sit beside their file as `*.test.ts(x)` and run with `npm test`.
Browser tests are in `e2e/`, against the mock API (`mock/`) or a real service
(`e2e/real`).
White-box tests and test/setup/support fixtures retain access to internals;
test-only helpers are not product exports.

## Known limitations

- Features other than `usage` and `platform` still import each other's internals, most often
  `panels` and `thread`. Each such
  import is a candidate for moving the shared piece to its real owner.
- `src/lib` still holds project image and identity helpers used by both `ui` and
  `settings`.
- The shared-folder inward rule covers static `import` and `export … from` for
  all features, and literal dynamic imports and type queries into `usage` and `platform`.
  Computed dynamic paths and white-box test/fixture imports are not checked.
