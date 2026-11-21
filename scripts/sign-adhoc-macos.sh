#!/bin/bash
# Signs a local macOS bundle ad hoc, the way release.yml does when it has no Apple identity
# (signingIdentity "-", hardenedRuntime false): the whole bundle gets one seal, so
# `codesign --verify --deep --strict` passes. Run it after `tauri build` (npm run build in desktop/).
# Usage: scripts/sign-adhoc-macos.sh [path/to/Orbyn Engineering.app]
set -euo pipefail
app="${1:-$(dirname "$0")/../desktop/src-tauri/target/release/bundle/macos/Orbyn Engineering.app}"
[ -d "$app" ] || { echo "sign-adhoc-macos: no app bundle at $app" >&2; exit 1; }
# No hardened runtime: ad hoc builds are not notarized, and the engine host (a Node executable) needs
# entitlements only under the hardened runtime. Sidecars are signed inside-out by --deep.
codesign --force --deep --sign - "$app"
codesign --verify --deep --strict --verbose=2 "$app"
echo "Ad hoc signed and verified: $app"
