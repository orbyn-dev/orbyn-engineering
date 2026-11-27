# Releasing

How a release is built today. Nothing here publishes: a person reviews and promotes the uploaded files.

## Jobs

- `.github/workflows/release.yml`, run by hand (`workflow_dispatch`), `release` environment.
- One job per target: macOS aarch64 and x86_64 (`app,dmg`), Windows x86_64 and aarch64 (`nsis`).
- Each job builds with updater files, checks the bundle, packs the CLI, writes checksums and an inventory (`scripts/release-evidence.mjs`), and uploads a 14-day artifact.
- `ci.yml` also builds a desktop bundle (`Unsigned desktop build`) on pull requests and pushes to main; on macOS it is ad-hoc signed with the same config and verified.

## What is signed

| Platform | State |
| --- | --- |
| macOS, `APPLE_*` secrets present | Tauri signs with them; `scripts/verify-macos-release.sh` checks notarization for a Developer ID identity. |
| macOS, no secrets (today) | Ad-hoc signed (`-`), whole bundle. Not notarized. |
| Windows | Not signed. The Azure step only reports that. No Authenticode signing exists yet. |

A local build is ad-hoc signed the same way by `scripts/sign-adhoc-macos.sh` (run it after `npm run build` in `desktop/`; it signs the whole `.app` with `-`, no hardened runtime, and verifies it). Without it a local bundle fails `codesign --verify --deep --strict` ("code has no resources but signature indicates they must be present"), because Tauri does not sign a bundle when it has no identity.

`scripts/sign-macos-release.sh` is the per-binary release signer (the host with `engine-host/entitlements.plist`, everything else with none); no release job runs it yet.

In both macOS cases `codesign --verify --deep --strict` runs on the `.app` and the dmg's `.app`, and a failure fails the job.

There is no Apple Developer account, so no Developer ID and no notarization. Expect a Gatekeeper block on a downloaded Mac build and SmartScreen on the Windows installer. Tester steps: [first run on an unsigned build](setup.md#first-run-on-an-unsigned-build).

## Not built

- The hardened-runtime, `get-task-allow` and `disable-library-validation` release check that [the sign-in gate](account.md#sign-in-gate) relies on. Nothing in CI or `scripts/` runs it. Ad-hoc builds set no hardened runtime.
- Windows signing, notarization, an MSI or MSIX.

## Updates

The updater reads `ORBYN_UPDATE_PUBKEY` and `ORBYN_UPDATE_ENDPOINT` at compile time. No endpoint or key is configured, so an unsigned build has no update channel ("Updates aren't set up for this build."). The release build turns on updater files only when both `ORBYN_UPDATE_PUBKEY` and `TAURI_SIGNING_PRIVATE_KEY` exist; otherwise it builds without them. UNVERIFIED: a run of that path.

`tauri.conf.json` sets `plugins.updater.requireSignedVersion`, so the app only accepts a release
whose artifact signature names the version the endpoint announced. The endpoint response is not
signed, and without that binding a compromised endpoint could pair an older, validly signed
artifact with a higher version and force a downgrade. Two consequences for releases:

- Every release users can still update from must be signed by a Tauri CLI that records the version
  in the signature's trusted comment. Re-sign and re-publish anything signed before that.
- A release whose signature carries no version, or another version, is refused: the shell reports
  `app_update_failed` and never offers the download as ready.

## Release notes

The notes are written when a release is made. Two account changes the next one has to say, so they
reach people who already have a build:

- **No last seen.** Device rows do not say when a device was last seen, and `AccountDevice` has no
  `last_seen`: the Engine's `Device` has none, so nothing is stored, synced or shown for it
  ([account.md](account.md)).
- **Sign-up makes no live account before the emailed code, and never replaces one.** The password step
  only asks for one; the emailed code is what makes the account, and a sign-up for an address that
  already has an account ends as a wrong code, with that account untouched
  ([the threat model](account-threat-model.md)).

## Not yet verified on a real machine

Each item is UNVERIFIED: only CI has run.

- UNVERIFIED: `orbyn://` links open the app on macOS and Windows.
- UNVERIFIED: keep-awake (`caffeinate`, `SetThreadExecutionState`).
- UNVERIFIED: notifications in WKWebView and WebView2.
- UNVERIFIED: SmartScreen behaviour on the unsigned NSIS installer.
- UNVERIFIED: macOS Screen Recording, Accessibility and folder access are re-granted after an update.
- UNVERIFIED: Gatekeeper behaviour with the ad-hoc signed bundle on current macOS (Open Anyway, or "damaged").
- UNVERIFIED: the updater end to end, check, download, install, relaunch (issues #514 and #515).
