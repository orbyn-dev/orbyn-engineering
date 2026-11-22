#!/bin/bash
# Release check for docs/account-threat-model.md controls 6 and 7: the entitlements and hardened runtime of every
# binary in a macOS app bundle. Fails on anything not listed here.
#
#   orbyn-engine-host (a Node single executable): may carry only allow-jit, allow-unsigned-executable-memory and
#     disable-library-validation. V8 needs the first two to run; the third lets the injected Node runtime load its own
#     code. Without them the host crashes at start (engine-host/README.md).
#   every other Mach-O (orbynd, orbyn, the app executable, frameworks): no entitlement at all. Above all not
#     get-task-allow or disable-library-validation, which would let a same-user process read orbynd's memory.
#   get-task-allow: never, on any binary.
#
# --release also requires the hardened runtime flag ("runtime") on every binary. Without it, hardened runtime is
# reported as skipped: an ad hoc build (signingIdentity "-", hardenedRuntime false) has none. Signing itself is blocked
# until there is an Apple Developer ID, so today this can only run against an ad hoc build; a Developer ID build must
# pass --release before it ships.
# Usage: scripts/check-macos-entitlements.sh [--release] <path/to/App.app>
set -euo pipefail

release=0
if [ "${1:-}" = "--release" ]; then release=1; shift; fi
app="${1:-}"
[ -d "$app" ] || { echo "usage: $0 [--release] <App.app>" >&2; exit 2; }

host_allowed="com.apple.security.cs.allow-jit com.apple.security.cs.allow-unsigned-executable-memory com.apple.security.cs.disable-library-validation"
failures=0
checked=0

fail() { echo "FAIL: $1" >&2; failures=$((failures + 1)); }

entitlement_keys() {
  # Prints one entitlement key per line; nothing for an unsigned binary or one with no entitlements.
  codesign -d --entitlements :- "$1" 2>/dev/null | python3 -c '
import plistlib, sys
data = sys.stdin.buffer.read()
start = data.find(b"<?xml")
if start < 0:
    sys.exit(0)
for key in plistlib.loads(data[start:]):
    print(key)
'
}

while IFS= read -r -d '' file; do
  case "$(file -b "$file")" in
    *Mach-O*) ;;
    *) continue ;;
  esac
  checked=$((checked + 1))
  name="$(basename "$file")"
  allowed=""
  [ "$name" = "orbyn-engine-host" ] && allowed="$host_allowed"
  while IFS= read -r key; do
    [ -n "$key" ] || continue
    if [ "$key" = "com.apple.security.get-task-allow" ]; then
      fail "$name carries com.apple.security.get-task-allow"
    elif [[ " $allowed " != *" $key "* ]]; then
      fail "$name carries $key, which is not allowed for it"
    fi
  done < <(entitlement_keys "$file")
  if [ "$release" = 1 ]; then
    codesign -dv "$file" 2>&1 | grep -Eq 'flags=0x[0-9a-f]+\(([a-z,]*,)?runtime(,[a-z,]*)?\)' \
      || fail "$name has no hardened runtime flag"
  fi
done < <(find "$app/Contents/MacOS" "$app/Contents/Frameworks" -type f -print0 2>/dev/null)

[ "$checked" -gt 0 ] || { echo "FAIL: no Mach-O binary found in $app" >&2; exit 1; }
if [ "$release" = 0 ]; then echo "Hardened runtime: not checked (no --release; an ad hoc build has none)."; fi
if [ "$failures" -gt 0 ]; then
  echo "$failures problem(s) in $checked binaries." >&2
  exit 1
fi
echo "Entitlements ok: $checked binaries checked."
