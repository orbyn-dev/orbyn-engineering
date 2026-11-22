# Open-source release checklist

For the owner. The Claude and Cursor SDKs may be used on the condition that Orbyn Engineering is open source for public use
(docs/sdk-default-plan.md). **The licence is the owner's choice**; this lists what has to be true, with what the repository shows
today. Nothing here is a legal opinion.

## What is true today

| Item | State |
| --- | --- |
| Repository visibility | `orbyn-dev/orbyn-engineering` is public. |
| Licence | `LICENSE` is PolyForm Strict 1.0.0 (source-available). The owner has said the licence does not matter for the SDK permissions: they are already granted, so the licence is not a gate for the SDK work. It stays the owner's choice for the project itself. |
| Engine dependency | `orbyn-dev/orbyn-engine` is **public** (Apache-2.0), not private. The app depends on it through one optional git dependency, `orbyn-device-client` at tag `engine-v0.2.4` (revision `db006e3bdf4e5dc411af1464665f63c8679172ce`), which brings the crates `orbyn-auth-client`, `orbyn-client`, `orbyn-contracts`, `orbyn-crypto`, `orbyn-device-client`, `orbyn-sync` and `orbyn-translog` from the same repository. No vendoring is needed. |
| Contributors | A CLA check runs on pull requests (`cla / CLA`). Check that it covers relicensing under the new licence. |
| Third-party notices | None shipped in the app bundle yet; SDK notices come with step 3 of the plan. |

## Checklist

1. **Choose the licence** (owner). Replace `LICENSE` and `license-file = "../../LICENSE"` in `desktop/src-tauri/Cargo.toml`;
   set `license` in each `Cargo.toml` and `package.json` that names one ("SEE LICENSE IN LICENSE" in `desktop/package.json` and
   `engine-host/package.json`); update README and CONTRIBUTING wording.
2. **Contributor rights.** Confirm every contributor (including outside ones) has agreed to the CLA or can be asked to; code
   adapted from MIT sources keeps its notice (UPSTREAM.md lists them).
3. **Third-party notices.** `THIRD_PARTY_NOTICES` for Rust crates, npm packages and the two SDKs with the files their licences
   ask for, shipped in the app and in the repository (plan step 3).
4. **Secrets and history.** Scan the whole git history for secrets, tokens, personal data and private design files before the
   licence changes; rotate anything found. Check `orbyn-references/`, `openSrc/` and `output/` are not tracked.
5. **Private references.** Search docs, scripts and workflows for private repository names, internal URLs and the private
   design repository; remove or replace. The private designs are never published (CONTRIBUTING.md).
6. **Trademark and names.** Decide the policy for the Orbyn name and logo separately from the code licence.
7. **Project files.** `SECURITY.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md` current; issue and PR templates public-safe.
8. **Release gate.** None from the licence: the SDK permissions are already granted (owner, 2026-11-22). Follow plan steps 3 to 6.
9. **Written permissions.** The owner keeps the Anthropic and Cursor permissions; they are not copied into the repository.
   Re-read their conditions before the first public release and note the date here.
