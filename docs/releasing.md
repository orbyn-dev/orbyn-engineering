# Releasing

How a release is built today. Nothing here publishes: a person reviews and promotes the uploaded files.

## Jobs

- `.github/workflows/release.yml`, run by hand (`workflow_dispatch`), `release` environment.
- One job per target: macOS aarch64 and x86_64 (`app,dmg`), Windows x86_64 and aarch64 (`nsis`).
- Each job builds with updater files, checks the bundle, packs the CLI, writes checksums and an inventory (`scripts/release-evidence.mjs`), and uploads a 14-day artifact.
- Pull requests only build an unsigned desktop bundle in `ci.yml` (`Unsigned desktop build`), ad-hoc signed and verified on macOS.

## What is signed

| Platform | State |
| --- | --- |
| macOS, `APPLE_*` secrets present | Tauri signs with them; `scripts/verify-macos-release.sh` checks notarization for a Developer ID identity. |
| macOS, no secrets (today) | Ad-hoc signed (`-`), whole bundle. Not notarized. `codesign --verify --deep --strict` must pass or the job fails. |
| Windows | Not signed. The Azure step only reports that. No Authenticode signing exists yet. |

There is no Apple Developer account, so no Developer ID and no notarization. Expect a Gatekeeper block on a downloaded Mac build and SmartScreen on the Windows installer. Tester steps: [first run on an unsigned build](setup.md#first-run-on-an-unsigned-build).

## Not built

- The hardened-runtime, `get-task-allow` and `disable-library-validation` release check that [the sign-in gate](account.md#sign-in-gate) relies on. Nothing in CI or `scripts/` runs it. Ad-hoc builds set no hardened runtime.
- Windows signing, notarization, an MSI or MSIX.

## Updates

The updater reads `ORBYN_UPDATE_PUBKEY` and `ORBYN_UPDATE_ENDPOINT` at compile time. No endpoint or key is configured, so an unsigned build has no update channel ("Updates aren't set up for this build."). `createUpdaterArtifacts` is on in the release build and needs `TAURI_SIGNING_PRIVATE_KEY`; whether the job passes with that secret empty is UNVERIFIED.

## Not yet verified on a real machine

Each item is UNVERIFIED: only CI has run.

- UNVERIFIED: `orbyn://` links open the app on macOS and Windows.
- UNVERIFIED: keep-awake (`caffeinate`, `SetThreadExecutionState`).
- UNVERIFIED: notifications in WKWebView and WebView2.
- UNVERIFIED: SmartScreen behaviour on the unsigned NSIS installer.
- UNVERIFIED: macOS Screen Recording, Accessibility and folder access are re-granted after an update.
- UNVERIFIED: Gatekeeper behaviour with the ad-hoc signed bundle on current macOS (Open Anyway, or "damaged").
- UNVERIFIED: the updater end to end, check, download, install, relaunch (issues #514 and #515).
