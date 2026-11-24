#!/bin/bash
# Signs a built macOS app bundle for release, per binary: only orbyn-engine-host gets engine-host/entitlements.plist;
# the app and every other binary keep the default hardened runtime with no entitlements.
#
# Why per binary: Tauri's bundle.macOS.entitlements is one file, and Tauri applies it to the app and every sidecar, so it
# cannot give the host its three entitlements without also giving them to orbynd. tauri.conf.json no longer sets it.
#
# The order is inside-out: the engine SDK native files, then the other sidecars, then the host, then the bundle itself.
# The bundle seal is made last because it hashes every binary it covers; signing one after it would break the bundle.
#
# Run it after `tauri build` and BEFORE the dmg and the updater archive are made from the bundle, so both are made from
# the signed bundle. No release job calls it yet: Developer ID signing is not available (#876, docs/releasing.md).
#
# Usage: scripts/sign-macos-release.sh [--identity <name>] <path/to/App.app>
set -euo pipefail

usage="Usage: scripts/sign-macos-release.sh [--identity <name>] <path/to/App.app>"
identity="${APPLE_SIGNING_IDENTITY:--}"
app=""

while [ "$#" -gt 0 ]; do
  case "$1" in
    --identity)
      [ "$#" -ge 2 ] || { echo "$usage" >&2; exit 2; }
      identity="$2"
      shift 2
      ;;
    -*)
      echo "$usage" >&2
      exit 2
      ;;
    *)
      [ -z "$app" ] || { echo "$usage" >&2; exit 2; }
      app="$1"
      shift
      ;;
  esac
done

[ -n "$app" ] || { echo "$usage" >&2; exit 2; }
[ -d "$app" ] || { echo "sign-macos-release: not a directory: $app" >&2; echo "$usage" >&2; exit 2; }

# A Developer ID signature needs a secure timestamp; an ad hoc one cannot have a timestamp. Never --timestamp=none.
codesign_args=(--force --options runtime)
if [ "$identity" != "-" ]; then
  codesign_args+=(--timestamp)
fi
codesign_args+=(--sign "$identity")

sign_file() {
  codesign "${codesign_args[@]}" "$@"
}

# a. The engine SDKs' native files are resources, so the bundle seal does not sign them by itself; frameworks, if any, go first too.
while IFS= read -r -d '' file; do
  case "$(file -b "$file")" in
    *Mach-O*)
      sign_file "$file"
      echo "Signed (no entitlements): $file"
      ;;
  esac
done < <(find "$app/Contents/Resources/sdk" "$app/Contents/Frameworks" -type f -print0 2>/dev/null)

# b. Every other Mach-O in MacOS except the main executable (the bundle's signature signs it) and the host.
main_executable="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$app/Contents/Info.plist")"
while IFS= read -r -d '' file; do
  name="$(basename "$file")"
  if [ "$name" = "$main_executable" ] || [ "$name" = "orbyn-engine-host" ]; then
    continue
  fi
  case "$(file -b "$file")" in
    *Mach-O*)
      sign_file "$file"
      echo "Signed (no entitlements): $file"
      ;;
  esac
done < <(find "$app/Contents/MacOS" -type f -print0 2>/dev/null)

# c. The host is the only binary that gets the entitlements.
host="$app/Contents/MacOS/orbyn-engine-host"
if [ -f "$host" ]; then
  sign_file --entitlements "$(dirname "$0")/../engine-host/entitlements.plist" "$host"
  echo "Signed (engine-host entitlements): $host"
fi

# d. The bundle last, with no --deep and no entitlements: its seal covers the binaries signed above.
codesign "${codesign_args[@]}" "$app"
echo "Signed bundle: $app"

# e. The release check is the gate; exit with its status.
status=0
"$(dirname "$0")/check-macos-entitlements.sh" --release "$app" || status=$?
exit "$status"
