#!/usr/bin/env python3
"""Checks that every required status check in ci.yml is produced by exactly one job.

Branch protection waits for these names. A job that GitHub skips at the job level
reports its name unexpanded when it is a matrix job (`Rust (${{ matrix.os }})`), so
the required name would never appear and the pull request could not merge. ci.yml
therefore gives each required name to a small aggregator job that always runs, and
this check keeps it that way:

- each required name is the `name:` of exactly one job;
- that job has no `${{` in its name and runs with `if: always()`;
- no other job's name, with its `${{ ... }}` parts read as any text, can expand to a
  required name.

Run `python3 scripts/check_ci_required_names.py [path]`; the exit status is 0 when
the file fits and otherwise 1 with one line per problem. It reads job names with
regular expressions, so it needs no YAML library.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

# The CI checks branch protection requires. The AI usage section, commit trailers and
# CLA checks come from other workflows.
REQUIRED = (
    "Rust (windows-2022)",
    "Web",
    "Unsigned desktop build (windows-2022)",
    "Rust (macos-15)",
    "Unsigned desktop build (macos-15)",
    "E2E",
)

DEFAULT_PATH = Path(__file__).resolve().parent.parent / ".github" / "workflows" / "ci.yml"

JOB_ID = re.compile(r"^  ([A-Za-z0-9_-]+):\s*(?:#.*)?$")
FIELD = re.compile(r"^    (name|if):\s*(.*?)\s*$")


def _unquote(value: str) -> str:
    if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
        return value[1:-1]
    return value


def read_jobs(text: str) -> dict[str, dict[str, str]]:
    """Return {job id: {"name": ..., "if": ...}} for the jobs in a workflow file."""
    jobs: dict[str, dict[str, str]] = {}
    in_jobs = False
    current: dict[str, str] | None = None
    for line in text.splitlines():
        if re.match(r"^\S", line):
            in_jobs = line.startswith("jobs:")
            current = None
            continue
        if not in_jobs:
            continue
        found = JOB_ID.match(line)
        if found:
            current = jobs.setdefault(found.group(1), {})
            continue
        field = FIELD.match(line)
        if field and current is not None:
            current[field.group(1)] = _unquote(field.group(2))
    return jobs


def _can_expand_to(template: str, name: str) -> bool:
    parts = re.split(r"\$\{\{.*?\}\}", template)
    pattern = ".+".join(re.escape(part) for part in parts)
    return re.fullmatch(pattern, name) is not None


def check(text: str, required: tuple[str, ...] = REQUIRED) -> list[str]:
    jobs = read_jobs(text)
    problems: list[str] = []
    for name in required:
        owners = [job for job, fields in jobs.items() if fields.get("name") == name]
        if len(owners) != 1:
            problems.append(f"required check {name!r} is produced by {len(owners)} jobs, not 1: {owners}")
        for job in owners:
            if "${{" in name:
                problems.append(f"job {job!r}: the required name {name!r} must not be an expression")
            if jobs[job].get("if", "").strip() not in ("always()", "${{ always() }}"):
                problems.append(f"job {job!r} produces required check {name!r} and must run with `if: always()`")
        for job, fields in jobs.items():
            template = fields.get("name", "")
            if job not in owners and "${{" in template and _can_expand_to(template, name):
                problems.append(f"job {job!r} named {template!r} can expand to the required name {name!r}")
    return problems


def main(argv: list[str]) -> int:
    path = Path(argv[1]) if len(argv) > 1 else DEFAULT_PATH
    problems = check(path.read_text(encoding="utf-8"))
    for problem in problems:
        print(f"{path}: {problem}")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
