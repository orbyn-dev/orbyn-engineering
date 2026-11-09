#!/usr/bin/env python3
"""Rust architecture guardrails (docs/refactor.md). Standard library only.

    python3 scripts/check_rust_arch.py            fail on NEW violations only
    python3 scripts/check_rust_arch.py --update   rewrite scripts/rust-arch-baseline.json

Rules:
  * a .rs file over 1500 lines may not appear (existing ones are in the baseline;
    one that grows past its baselined size is reported as a warning);
  * crates/orbynd/src may not gain a `Command::new("git")` call, nor more
    `crate::store` / `orbyn_core::store` references than its baseline (the service
    is transport only; git and persistence belong to orbyn-core).
"""

import json
import re
import sys
from pathlib import Path

MAX_LINES = 1500
ROOT = Path(__file__).resolve().parent.parent
BASELINE = Path(__file__).resolve().parent / "rust-arch-baseline.json"
GIT_SPAWN = re.compile(r'Command::new\(\s*"git"\s*\)')
STORE_USE = re.compile(r"\b(?:crate|orbyn_core)::store\b")
ORBYND = "crates/orbynd/src/"


def count_lines(text):
    return text.count("\n") + (1 if text and not text.endswith("\n") else 0)


def scan(files):
    """files: {relative posix path: text}. Returns the current measurements."""
    long_files, git, store = {}, {}, {}
    for rel, text in sorted(files.items()):
        if not rel.endswith(".rs"):
            continue
        n = count_lines(text)
        if n > MAX_LINES:
            long_files[rel] = n
        if rel.startswith(ORBYND):
            if found := len(GIT_SPAWN.findall(text)):
                git[rel] = found
            if found := len(STORE_USE.findall(text)):
                store[rel] = found
    return {"long_files": long_files, "orbynd_git_spawns": git, "orbynd_store_refs": store}


def compare(current, baseline):
    """Returns (failures, warnings)."""
    failures, warnings = [], []
    known = baseline.get("long_files", {})
    for rel, n in current["long_files"].items():
        if rel not in known:
            failures.append(f"{rel} has {n} lines; new Rust files stay under {MAX_LINES}")
        elif n > known[rel]:
            warnings.append(f"{rel} grew from {known[rel]} to {n} lines")
    for key, what in (("orbynd_git_spawns", 'Command::new("git")'), ("orbynd_store_refs", "a core store reference")):
        known = baseline.get(key, {})
        for rel, n in current[key].items():
            if n > known.get(rel, 0):
                failures.append(f"{rel} gains {what} ({known.get(rel, 0)} allowed, {n} found); move it into orbyn-core")
    return failures, warnings


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
    for w in warnings:
        print(f"warning: {w}")
    if failures:
        print("rust arch: new violation(s) (docs/refactor.md):", file=sys.stderr)
        for f in failures:
            print(f"  {f}", file=sys.stderr)
        return 1
    print("rust arch: no new violations")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
