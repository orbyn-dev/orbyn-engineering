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
# The engine SDKs' native files (Contents/Resources/sdk, Mach-O) are resources to --deep, so they are signed first, one by
# one, with no entitlements; the bundle seal then covers them.
while IFS= read -r -d '' file; do
  case "$(file -b "$file")" in
    *Mach-O*) codesign --force --sign - "$file" ;;
  esac
done < <(find "$app/Contents/Resources/sdk" -type f -print0 2>/dev/null)
codesign --force --deep --sign - "$app"
codesign --verify --deep --strict --verbose=2 "$app"
echo "Ad hoc signed and verified: $app"
