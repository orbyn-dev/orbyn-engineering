#!/usr/bin/env python3
"""Writes an advisory timing table for a finished CI run to the GitHub step summary.

Advisory only: it NEVER fails. Whatever goes wrong, including an API error, it prints the
error in the summary and exits 0. It writes the summary and `::warning::` lines on stdout
and nothing else: no comments, no labels, no API writes.

The summary has the wall time of the run (run_started_at to updated_at) and, for the six
slowest jobs, queue time (created to started), run time (started to completed) and the
runner type: hosted, or self-hosted for the orbyn-local runner. A job with a missing
timestamp (never started, or skipped) is left out. A `::warning::` is printed when the wall
time is over the budget.

Every GitHub call goes through gh() below, so tests inject a fake. Stdlib only.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from datetime import datetime
from typing import Callable

# Minutes a run should finish within. This repeats the "Time budget" in the header comment
# of .github/workflows/ci.yml ("about 8 minutes"); it is not read from there, because a
# comment is not a stable interface. Change both together.
BUDGET_MINUTES = 8
SLOWEST = 6
PAGE = 100
MAX_PAGES = 50

Gh = Callable[..., str]


class GhError(RuntimeError):
    pass


def gh(args: list[str], stdin: str | None = None) -> str:
    """The only place that runs the gh CLI. Returns stdout; raises GhError on failure."""
    try:
        done = subprocess.run(
            ["gh", *args], input=stdin, capture_output=True, text=True, timeout=60, check=False
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise GhError(f"gh {' '.join(args[:3])}: {exc}") from exc
    if done.returncode != 0:
        raise GhError(f"gh {' '.join(args[:3])}: {(done.stderr or done.stdout).strip()}")
    return done.stdout


def parse_time(value: object) -> datetime | None:
    if not isinstance(value, str) or not value:
        return None
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return None


def fmt(seconds: float) -> str:
    seconds = max(0, round(seconds))
    return f"{seconds // 60}m {seconds % 60:02d}s"


def runner_type(job: dict) -> str:
    labels = job.get("labels") or []
    if "self-hosted" in labels or "orbyn-local" in labels:
        return "self-hosted (orbyn-local)" if "orbyn-local" in labels else "self-hosted"
    return "hosted"


def cell(text: object) -> str:
    """Untrusted text for a Markdown table cell: one line, no pipes, no markup."""
    clean = re.sub(r"[\x00-\x1f\x7f]+", " ", str(text)).strip()
    return re.sub(r"[|`<>\\]", "_", clean)[:120] or "-"


def timed_jobs(jobs: list[dict]) -> list[dict]:
    rows = []
    for job in jobs:
        created = parse_time(job.get("created_at"))
        started = parse_time(job.get("started_at"))
        done = parse_time(job.get("completed_at"))
        if not (created and started and done) or job.get("conclusion") == "skipped":
            continue
        rows.append({
            "name": job.get("name") or "(unnamed)",
            "queue": (started - created).total_seconds(),
            "run": (done - started).total_seconds(),
            "runner": runner_type(job),
            "conclusion": job.get("conclusion") or "-",
        })
    return sorted(rows, key=lambda r: r["run"], reverse=True)[:SLOWEST]


def report(run: dict, jobs: list[dict], event: str | None = None) -> tuple[str, list[str]]:
    """The summary Markdown and the annotation lines.

    The budget is the pull request budget of ci.yml: a push to main runs the full suite on
    every OS (about 29 minutes by design), so only a pull request run, or a run with no
    event given, can warn.
    """
    start, end = parse_time(run.get("run_started_at")), parse_time(run.get("updated_at"))
    lines = [f"## CI timing (advisory) for run {int(run['id'])}", ""]
    annotations: list[str] = []
    if start and end:
        wall = (end - start).total_seconds()
        applies = event in (None, "pull_request")
        note = f"budget {BUDGET_MINUTES}m 00s" if applies else f"the {BUDGET_MINUTES}m budget applies to pull requests, not {cell(event)} runs"
        lines.append(f"Wall time: **{fmt(wall)}** ({note})")
        if applies and wall > BUDGET_MINUTES * 60:
            annotations.append(
                f"::warning title=CI over its time budget::Run {int(run['id'])} took {fmt(wall)}, "
                f"over the {BUDGET_MINUTES} minute budget in ci.yml."
            )
    else:
        lines.append("Wall time: unknown (the run has no start or end time).")
    rows = timed_jobs(jobs)
    lines += ["", f"Slowest {SLOWEST} jobs by run time:", ""]
    if rows:
        lines += ["| Job | Queue | Run | Runner | Result |", "| --- | ---: | ---: | --- | --- |"]
        for r in rows:
            lines.append(f"| {cell(r['name'])} | {fmt(r['queue'])} | {fmt(r['run'])} | {r['runner']} | {cell(r['conclusion'])} |")
    else:
        lines.append("No job has complete timestamps.")
    return "\n".join(lines) + "\n", annotations


def fetch(run_gh: Gh, repo: str, run_id: int) -> tuple[dict, list[dict]]:
    run = json.loads(run_gh(["api", f"repos/{repo}/actions/runs/{run_id}"]))
    jobs: list[dict] = []
    for page in range(1, MAX_PAGES + 1):
        path = f"repos/{repo}/actions/runs/{run_id}/jobs?filter=latest&per_page={PAGE}&page={page}"
        items = json.loads(run_gh(["api", path]))["jobs"]
        jobs += items
        if len(items) < PAGE:
            break
    return run, jobs


def main(argv: list[str] | None = None, run_gh: Gh = gh, out=print, summary_path: str | None = None) -> int:
    """Always returns 0."""
    try:
        parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
        parser.add_argument("--run-id", required=True)
        parser.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY", ""))
        parser.add_argument("--event", default=None, help="event of the CI run; only pull_request can warn")
        args = parser.parse_args(argv)
    except SystemExit:
        text, notes = "## CI timing (advisory)\n\nCould not read the arguments.\n", []
    else:
        try:
            if not re.fullmatch(r"[0-9]{1,20}", args.run_id) or not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", args.repo):
                raise ValueError("invalid run id or repository")
            run, jobs = fetch(run_gh, args.repo, int(args.run_id))
            text, notes = report(run, jobs, args.event)
        except Exception as exc:  # advisory: any failure becomes text, never a failed workflow
            message = re.sub(r"\s+", " ", f"{type(exc).__name__}: {exc}")[:300]
            text, notes = f"## CI timing (advisory)\n\nCould not build the timing report: {cell(message)}\n", []
    try:
        path = summary_path if summary_path is not None else os.environ.get("GITHUB_STEP_SUMMARY")
        if path:
            with open(path, "a", encoding="utf-8") as handle:
                handle.write(text)
        else:
            out(text)
        for note in notes:
            out(note)
    except Exception as exc:
        out(f"ci_timing: could not write the summary: {type(exc).__name__}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
