#!/usr/bin/env python3
"""Rust architecture guardrails (docs/refactor.md). Standard library only.

    python3 scripts/check_rust_arch.py            fail on NEW violations only
    python3 scripts/check_rust_arch.py --update   rewrite scripts/rust-arch-baseline.json

Rules (the baseline may only shrink):
  * a .rs file over 1500 lines may not appear, and a baselined one may not grow;
  * crates/orbynd/src may not gain a `Command::new("git")` call, nor more
    `crate::store` / `orbyn_core::store` references than its baseline, including
    grouped imports such as `use orbyn_core::{Core, store::Store}` (the service is
    transport only; git and persistence belong to orbyn-core);
  * every `Command::new` and `CommandBuilder::new` in non-test code must sit in a function that routes
    through `host::child_env` (`for_tool`, `for_engine` or `for_terminal`), the one place that scrubs
    what a child inherits (account secrets among it, docs/account-threat-model.md control 2), unless
    it is one of the audited `SPAWN_EXCEPTIONS` below, each with its reason. An exception that no
    longer applies fails, so the list only shrinks;
  * a baselined item that no longer exists (file gone or under the limit, reference
    removed) fails, so re-run `--update` and the baseline shrinks. A file that shrinks
    but stays long only prints a notice.

Heuristic gaps: this is text matching, not a parser. It misses a git spawn whose
program name sits in a const or variable (`Command::new(GIT)`), a spawn through a
helper, a `use` that renames the module, and `store` reached through a glob import.
Comments are stripped first, and `#[cfg(test)] mod x { ... }` blocks are dropped (a test fixture
spawns what it likes). The routing check is per function: it finds the function around a spawn by
indentation and looks for the routing call in its text, so a spawn that is routed by a helper in
another function needs an exception or a rewrite. CI runs it on Ubuntu only; it uses only the standard library.
"""

import json
import re
import sys
from pathlib import Path

MAX_LINES = 1500
ROOT = Path(__file__).resolve().parent.parent
BASELINE = Path(__file__).resolve().parent / "rust-arch-baseline.json"
GIT_SPAWN = re.compile(r'Command::new\(\s*"git"\s*\)')
STORE_USE = re.compile(r"\b(?:crate|orbyn_core)::store\b|\b(?:crate|orbyn_core)::\{[^;]*?\bstore\b", re.S)
COMMENTS = re.compile(r"/\*.*?\*/|(?<!:)//[^\n]*", re.S)
ORBYND = "crates/orbynd/src/"
SPAWN = re.compile(r"\b(?:Command|CommandBuilder)::new\(")
ROUTED = re.compile(r"\bfor_(?:tool|engine|terminal)\b")
FN = re.compile(r"^(\s*)(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(?:const\s+)?fn\s")
TEST_MOD = re.compile(r"#\[cfg\([^\]]*\btest\b[^\]]*\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*\{")
# Audited spawns that do not go through `host::child_env`, by path prefix: (how many, why). A new one is
# a decision, not a convenience; a path that no longer spawns must be removed from here.
SPAWN_EXCEPTIONS = {
    "crates/orbyn-core/src/crew/autopilot/merge/gitops.rs": (
        1,
        "a best-effort `git worktree remove` in Drop; the crew module is owned by the crew work and routes it next",
    ),
    "crates/orbynd/src/bin/orbyn_fake_engine/": (
        2,
        "the fake engine binary used by tests: it starts fixtures and is never shipped",
    ),
}


def count_lines(text):
    return text.count("\n") + (1 if text and not text.endswith("\n") else 0)


def strip_test_modules(text):
    """Drops every `#[cfg(test)] mod x { ... }` block (braces counted; strings with braces are rare here)."""
    while match := TEST_MOD.search(text):
        depth, i = 1, match.end()
        while i < len(text) and depth:
            depth += {"{": 1, "}": -1}.get(text[i], 0)
            i += 1
        text = text[: match.start()] + text[i:]
    return text


def enclosing_function(lines, at):
    """The text of the function around line `at`, by indentation, or None outside any function."""
    indent = len(lines[at]) - len(lines[at].lstrip())
    start = at
    while start >= 0:
        found = FN.match(lines[start])
        if found and len(found.group(1)) < indent:
            break
        start -= 1
    if start < 0:
        return None
    base = len(FN.match(lines[start]).group(1))
    end = start + 1
    while end < len(lines) and not (lines[end].startswith(" " * base + "}") and len(lines[end]) - len(lines[end].lstrip()) == base):
        end += 1
    return "\n".join(lines[start : end + 1])


def unrouted_spawns(rel, text):
    """The line numbers of spawns in this non-test file whose function never calls `child_env`."""
    if "/src/" not in rel or any(part in ("tests", "testing") for part in rel.split("/")) or rel.endswith(("/tests.rs", "/testing.rs", "_tests.rs", "/fixtures.rs")):
        return []
    lines = strip_test_modules(text).split("\n")
    found = []
    for at, line in enumerate(lines):
        if SPAWN.search(line):
            body = enclosing_function(lines, at)
            if body is None or not ROUTED.search(body):
                found.append(at + 1)
    return found


def exception_for(rel):
    return next((key for key in SPAWN_EXCEPTIONS if rel.startswith(key)), None)


def scan(files):
    """files: {relative posix path: text}. Returns the current measurements."""
    long_files, git, store, spawns = {}, {}, {}, {}
    for rel, raw in sorted(files.items()):
        if not rel.endswith(".rs"):
            continue
        n = count_lines(raw)
        text = COMMENTS.sub("", raw)
        if n > MAX_LINES:
            long_files[rel] = n
        if found := unrouted_spawns(rel, text):
            spawns[rel] = len(found)
        if rel.startswith(ORBYND):
            if found := len(GIT_SPAWN.findall(text)):
                git[rel] = found
            if found := len(STORE_USE.findall(text)):
                store[rel] = found
    return {"long_files": long_files, "orbynd_git_spawns": git, "orbynd_store_refs": store, "unrouted_spawns": spawns}


def compare(current, baseline):
    """Returns (failures, notices). Stale baseline entries fail; a shrink only notices."""
    failures, notices = [], []
    known = baseline.get("long_files", {})
    for rel, n in current["long_files"].items():
        if rel not in known:
            failures.append(f"{rel} has {n} lines; new Rust files stay under {MAX_LINES}")
        elif n > known[rel]:
            failures.append(f"{rel} grew from {known[rel]} to {n} lines")
        elif n < known[rel]:
            notices.append(f"{rel} shrank from {known[rel]} to {n} lines; run --update to lower it")
    for rel in known:
        if rel not in current["long_files"]:
            failures.append(f"{rel} is gone or under {MAX_LINES} lines; run --update so the baseline shrinks")
    seen = {}
    for rel, n in current.get("unrouted_spawns", {}).items():
        key = exception_for(rel)
        if key is None:
            failures.append(f"{rel} spawns a process ({n}) outside a function that calls host::child_env (for_tool, for_engine or for_terminal); route it, or audit it into SPAWN_EXCEPTIONS")
        else:
            seen[key] = seen.get(key, 0) + n
    for key, (allowed, _why) in SPAWN_EXCEPTIONS.items():
        if seen.get(key, 0) > allowed:
            failures.append(f"{key} has {seen[key]} unrouted spawns ({allowed} audited)")
    for key, what in (("orbynd_git_spawns", 'Command::new("git")'), ("orbynd_store_refs", "a core store reference")):
        known = baseline.get(key, {})
        for rel, n in current[key].items():
            if n > known.get(rel, 0):
                failures.append(f"{rel} gains {what} ({known.get(rel, 0)} allowed, {n} found); move it into orbyn-core")
            elif n < known[rel]:
                notices.append(f"{rel} has fewer {what} ({n} of {known[rel]}); run --update")
        for rel in known:
            if rel not in current[key]:
                failures.append(f"{rel} no longer has {what}; run --update so the baseline shrinks")
    return failures, notices


def stale_exceptions(current):
    """Audited exceptions that no longer apply. Judged on the whole tree only, so a partial scan does not use it."""
    seen = {}
    for rel, n in current.get("unrouted_spawns", {}).items():
        if (key := exception_for(rel)) is not None:
            seen[key] = seen.get(key, 0) + n
    return [
        f"{key} no longer has {allowed} unrouted spawns ({seen.get(key, 0)} found); remove or lower its SPAWN_EXCEPTIONS entry"
        for key, (allowed, _why) in SPAWN_EXCEPTIONS.items()
        if seen.get(key, 0) < allowed
    ]


def read_tree(root):
    out = {}
    for path in (root / "crates").rglob("*.rs"):
        if "target" in path.relative_to(root).parts:
            continue
        out[path.relative_to(root).as_posix()] = path.read_text(encoding="utf-8")
    return out


def main(argv):
    current = scan(read_tree(ROOT))
    if "--update" in argv:
        body = {"_note": "Recorded Rust architecture debt (docs/refactor.md). Only shrink it.", "max_lines": MAX_LINES, **current}
        BASELINE.write_text(json.dumps(body, indent=2) + "\n", encoding="utf-8")
        print(f"rust arch baseline written: {len(current['long_files'])} long files")
        return 0
    failures, warnings = compare(current, json.loads(BASELINE.read_text(encoding="utf-8")))
    failures += stale_exceptions(current)
    for w in warnings:
        print(f"notice: {w}")
    if failures:
        print("rust arch: violation(s) (docs/refactor.md):", file=sys.stderr)
        for f in failures:
            print(f"  {f}", file=sys.stderr)
        return 1
    print("rust arch: no new violations")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
