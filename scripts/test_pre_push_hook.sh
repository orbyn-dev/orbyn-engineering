#!/bin/sh
# Tests .githooks/pre-push in a throwaway git repository with stub cargo and npm.
# Run: sh scripts/test_pre_push_hook.sh
set -u
here=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM
fails=0
passes=0

ok() { passes=$((passes + 1)); echo "ok   $1"; }
bad() { fails=$((fails + 1)); echo "FAIL $1"; }
assert() { # $1 = name, rest = test command
  name=$1; shift
  if "$@"; then ok "$name"; else bad "$name"; fi
}
has() { grep -q -- "$1" "$2"; }
lacks() { ! grep -q -- "$1" "$2"; }

# Stub tools: record every call; fail on request through the environment.
mkdir "$tmp/bin"
cat > "$tmp/bin/cargo" <<'EOF'
#!/bin/sh
echo "cargo $*" >> "$STUB_LOG"
[ -z "${STUB_FMT_FAIL:-}" ]
EOF
cat > "$tmp/bin/npm" <<'EOF'
#!/bin/sh
echo "npm $*" >> "$STUB_LOG"
[ -z "${STUB_NPM_FAIL:-}" ]
EOF
chmod +x "$tmp/bin/cargo" "$tmp/bin/npm"
export STUB_LOG="$tmp/calls.log"
export PATH="$tmp/bin:$PATH"

zeros=0000000000000000000000000000000000000000

# A repo with a bare origin, main pushed, and the hook copied in.
setup() {
  rm -rf "$tmp/origin.git" "$tmp/work"
  git init -q --bare "$tmp/origin.git"
  git init -q -b main "$tmp/work"
  (
    cd "$tmp/work" || exit 1
    git config user.email t@example.com; git config user.name t
    mkdir -p .githooks scripts crates/x web/src docs
    cp "$here/.githooks/pre-push" .githooks/pre-push
    for s in test_check_rust_arch check_rust_arch test_check_ci_required_names check_ci_required_names; do
      printf 'import os, sys\nsys.exit(int(os.environ.get("STUB_PY_FAIL") == "%s"))\n' "$s" > "scripts/$s.py"
    done
    echo base > crates/x/lib.rs; echo base > web/src/a.ts; echo base > docs/a.md
    git add -A; git commit -q -m base
    git remote add origin "$tmp/origin.git"
    git push -q origin main 2>/dev/null
    git fetch -q origin
    git checkout -q -b feat
  )
  : > "$STUB_LOG"
  unset STUB_FMT_FAIL STUB_NPM_FAIL STUB_PY_FAIL
}

# Commits a change to one file on the feature branch and runs the hook as git push would.
run_hook() { # $1 = file to change; output in $tmp/out, status in $?
  (
    cd "$tmp/work" || exit 1
    echo "change $$" >> "$1"
    git add -A; git commit -q -m change
    sha=$(git rev-parse HEAD)
    echo "refs/heads/feat $sha refs/heads/feat $zeros" | sh .githooks/pre-push
  ) > "$tmp/out" 2>&1
}

setup
STUB_FMT_FAIL=1 run_hook crates/x/lib.rs; code=$?
assert "formatting failure blocks" test "$code" -ne 0
assert "failing command is shown" has "FAILED: cargo fmt --all --check" "$tmp/out"
assert "new branch compares with origin/main" has "cargo fmt --all --check" "$tmp/out"

setup
run_hook crates/x/lib.rs; code=$?
assert "clean push passes" test "$code" -eq 0
assert "clean push ran cargo fmt" has "cargo fmt --all --check" "$STUB_LOG"
assert "never builds, tests or clippys" lacks "build\|test\|clippy" "$STUB_LOG"
assert "required-names check ran" has "check_ci_required_names" "$tmp/out"

setup
STUB_PY_FAIL=check_ci_required_names run_hook scripts/x.py; code=$?
assert "script check failure blocks" test "$code" -ne 0
assert "script failure is named" has "FAILED: python3 scripts/test_check_ci_required_names.py" "$tmp/out"

setup
run_hook web/src/a.ts; code=$?
assert "web without node_modules passes" test "$code" -eq 0
assert "web skip is announced with the reason" has "SKIPPED web check.*node_modules is missing" "$tmp/out"
assert "web skip does not run npm" lacks "npm" "$STUB_LOG"
assert "web only change skips rust checks" lacks "cargo" "$STUB_LOG"

setup
mkdir "$tmp/work/web/node_modules"
run_hook web/src/a.ts; code=$?
assert "web with node_modules runs npm run check" has "npm run check" "$STUB_LOG"
assert "web check passes when npm passes" test "$code" -eq 0
setup
mkdir "$tmp/work/web/node_modules"
STUB_NPM_FAIL=1 run_hook web/src/a.ts; code=$?
assert "web check failure blocks" test "$code" -ne 0

setup
run_hook docs/a.md; code=$?
assert "no relevant changes passes" test "$code" -eq 0
assert "no relevant changes runs nothing" test ! -s "$STUB_LOG"
assert "no relevant changes says so" has "nothing to run" "$tmp/out"

setup
(cd "$tmp/work" && sh .githooks/pre-push < /dev/null) > "$tmp/out" 2>&1; code=$?
assert "no changes by hand passes" test "$code" -eq 0

# End to end: git push honours core.hooksPath and --no-verify.
setup
(
  cd "$tmp/work" || exit 1
  git config core.hooksPath .githooks
  echo more >> crates/x/lib.rs; git add -A; git commit -q -m change
  STUB_FMT_FAIL=1 git push -q origin feat
) > "$tmp/out" 2>&1; code=$?
assert "git push is blocked by the hook" test "$code" -ne 0
assert "blocked push left origin without the branch" test -z "$(git -C "$tmp/origin.git" branch --list feat)"
(cd "$tmp/work" && STUB_FMT_FAIL=1 git push -q --no-verify origin feat) > "$tmp/out" 2>&1; code=$?
assert "--no-verify skips the hook" test "$code" -eq 0

# The installer sets and prints the undo.
setup
(cd "$tmp/work" && cp "$here/scripts/install-hooks.sh" scripts/ && sh scripts/install-hooks.sh) > "$tmp/out" 2>&1
assert "installer sets core.hooksPath" test "$(git -C "$tmp/work" config core.hooksPath)" = ".githooks"
assert "installer prints the undo" has "git config --unset core.hooksPath" "$tmp/out"

echo "$passes passed, $fails failed"
[ "$fails" -eq 0 ]
