#!/bin/sh
# Opts this clone in to the repository's git hooks (.githooks/pre-push).
set -eu
root=$(git rev-parse --show-toplevel)
cd "$root"
chmod +x .githooks/pre-push
git config core.hooksPath .githooks
echo "Set core.hooksPath to .githooks for the clone at $root."
echo "git push now runs the cheap checks of CI in .githooks/pre-push."
echo "Skip once: git push --no-verify"
echo "Undo:      git config --unset core.hooksPath"
