# Setup on macOS and Windows

Install Node.js 22.22.2 or later in the 22.x line (or 24.15.0 or later in
24.x, or 26 or later), npm 11.4.0, Rust 1.90 or later with [rustup](https://rustup.rs/),
Git and the [GitHub CLI](https://cli.github.com/). For native builds, install the
platform tools in [Tauri's prerequisites](https://v2.tauri.app/start/prerequisites/)
(Xcode Command Line Tools on macOS; MSVC build tools and WebView2 on Windows).
Use Terminal on macOS or PowerShell on Windows. Check the tools in a new terminal:

```text
node --version
npm --version
rustc --version
cargo --version
git --version
gh --version
```

## Clone and install

On macOS:

```bash
git clone https://github.com/orbyn-dev/orbyn-engineering.git
```

In PowerShell on Windows:

```powershell
git clone --config core.autocrlf=false https://github.com/orbyn-dev/orbyn-engineering.git
```

The Windows option preserves the repository's LF line endings: a stylesheet test
currently fails with CRLF checkout conversion. [Git applies this setting](https://git-scm.com/docs/git-clone)
to the new repository before its first checkout; it changes no global settings.
Then, in either shell:

```text
cd orbyn-engineering
cd web
npm ci
npm run test:e2e:install
```

The last command downloads Playwright's headless shell into `web/node_modules`.
The app and tests do not need an engine login for the mock API.

## Run with the mock

From `web`:

```text
npm run dev
```

Open the URL Vite prints (normally `http://localhost:5273`). Stop it with Ctrl+C.

## Run with the real service

From the repository root, select this checkout's own Cargo target and start the
service. Leave this terminal open. On macOS:

```bash
export CARGO_TARGET_DIR="$(git rev-parse --show-toplevel)/target"
cargo build -p orbynd
cargo run -p orbynd --bin orbynd -- serve --data-dir target/setup-data
```

In PowerShell:

```powershell
$env:CARGO_TARGET_DIR = Join-Path (git rev-parse --show-toplevel) 'target'
cargo build -p orbynd
cargo run -p orbynd --bin orbynd -- serve --data-dir target/setup-data
```

Development data stays in this checkout's ignored `target/setup-data`. The service
prints JSON with a `url` and `token`. In a second terminal, from
`web`, set `ORBYN_API` to that **url** and start Vite. On macOS:

```bash
export ORBYN_API='http://127.0.0.1:PORT'
npm run dev
```

In PowerShell:

```powershell
$env:ORBYN_API = 'http://127.0.0.1:PORT'
npm run dev
```

Replace the example with the service's actual URL. Open Vite's printed URL with
`?token=TOKEN` appended, substituting the token from the service output, for
example `http://localhost:5273/?token=TOKEN`. Keep the token private. Stop both
processes with Ctrl+C. To return to the mock in that terminal, unset `ORBYN_API`
(`unset ORBYN_API` on macOS, `Remove-Item Env:ORBYN_API` in PowerShell).

## Checks

From `web`, after installing dependencies and the headless shell, clear the real
API setting before running the mock spec. On macOS:

```bash
unset ORBYN_API
```

In PowerShell:

```powershell
Remove-Item Env:ORBYN_API -ErrorAction SilentlyContinue
```

Then run:

```text
npm run -s check
npm test
npx playwright test e2e/workspace.spec.ts
```

The example E2E uses the mock and requires port 5274. To run a real-service spec,
build the service from the root with the private target selected above:

```text
cargo build -p orbynd --bin orbynd
```

On Windows, from the repository root, set the executable path in the PowerShell
session that will run the test:

```powershell
$env:ORBYN_BIN = Join-Path (git rev-parse --show-toplevel) 'target/debug/orbynd.exe'
```

Then, from `web`, run a specific real-service spec:

```text
npx playwright test e2e/real/settings-search.spec.ts
```

See [contributor checks](../CONTRIBUTING.md#checks) for the Rust checks and the
required PR checks. The real-service fixture creates symbolic links; Windows
needs permission to create them (for example, Developer Mode). A single example
spec does not replace the full E2E suite.

## Unsigned desktop build

From the repository root, select its private `target` as above, then:

```text
cd desktop
npm ci
npm run prepare-bundle
```

This builds the web UI and service and fetches the pinned skills library; it needs
network access. `web/npm ci` must already have completed. Build an unsigned macOS
app bundle from `desktop`:

```bash
npx tauri build --bundles app
```

Or build an unsigned Windows installer from `desktop` in PowerShell:

```powershell
npx tauri build --bundles nsis
```

## When something fails

- **Port in use:** Vite can choose another port for development; use its printed
  URL. Playwright requires 5274 and refuses an existing server. Stop only a
  process you own, or free that port before rerunning.
- **Headless browser missing:** run `npm run test:e2e:install` from `web` again
  after `npm ci`. Use Playwright's shell, not an installed browser.
- **Shared Cargo target:** select this checkout's `target` in every terminal as
  shown above; do not run two checkouts against a writable target. If artifacts are
  suspect, follow [the recovery steps](../CONTRIBUTING.md#keep-cargo-artifacts-private-to-each-worktree).
- **Low disk space:** the browser, Rust targets and desktop bundle need space.
  Free space in artifacts you own before retrying; do not remove another worktree's
  outputs.
- **Windows paths or PowerShell:** run the PowerShell commands in PowerShell, not
  `cmd`; quote paths containing spaces. Start a new terminal if an installed tool
  is not on `PATH`, and check the platform prerequisites above.
