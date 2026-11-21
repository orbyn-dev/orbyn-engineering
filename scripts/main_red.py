#!/usr/bin/env python3
"""Opens, updates and closes one issue per incident when CI is red on main.

Nothing alerted when the CI run on main failed: E2E stayed red for 11 pushes unnoticed.
The Main red workflow runs this with the id of a finished CI run. It reads the run and
its jobs, then:

- failure: comments on the open `ci: main red` issue (once per run id), or opens one;
- success: comments "Green again" on the open issue and closes it; nothing when none is open;
- cancelled, skipped, neutral or not finished: nothing.

A run that is not on main, or that is a pull request run, is ignored: a fork can have a
branch called main. The run is read again by id, so a rerun that went green is seen as
green. Stdlib only. Every GitHub call goes through gh() below, so tests inject a fake.
`--dry-run` prints each write it would make and makes none (reads still happen).

Commit messages, pull request titles and job names are untrusted: they only enter a body
inside a code span (code_span), which GitHub renders literally and does not use for
@mentions, and bodies travel as JSON on stdin, never through a shell.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from typing import Callable
from urllib.parse import quote

LABEL = "ci: main red"
LABEL_COLOR = "b60205"
LABEL_DESCRIPTION = "CI failed on main; one issue per incident"
TITLE = "CI is red on main"
RED = ("failure", "timed_out", "startup_failure")
FAILED_JOB = ("failure", "timed_out")
PAGE = 100
# ci-retry.yml reruns a failed first attempt once when one of these macOS jobs failed, and the
# rerun fires its own completed event. Alerting on the first attempt would open an issue for
# every retried flake, so the first attempt waits for that rerun. Keep in step with ci-retry.yml.
RETRIED_JOBS = ("Desktop cache warm (macos-15)", "Desktop build (macos-15)")
MAX_PAGES = 50
MAX_JOBS_LISTED = 50
MAX_TEXT = 200

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


def api_get(run_gh: Gh, path: str):
    return json.loads(run_gh(["api", path]) or "null")


def pages(run_gh: Gh, path: str, key: str | None = None) -> list:
    """Every item of a paginated list endpoint, a page of PAGE at a time."""
    sep = "&" if "?" in path else "?"
    out: list = []
    for page in range(1, MAX_PAGES + 1):
        data = api_get(run_gh, f"{path}{sep}per_page={PAGE}&page={page}")
        items = data[key] if key else data
        out.extend(items)
        if len(items) < PAGE:
            break
    return out


def code_span(text: object) -> str:
    """Untrusted text as one inline code span: single line, no control characters.

    The fence is longer than any backtick run inside, so the text cannot close it.
    GitHub does not notify @mentions or link #refs inside code, so the text stays inert.
    """
    clean = re.sub(r"[\x00-\x1f\x7f\u200b-\u200f\u2028\u2029\u202a-\u202e\u2066-\u2069\ufeff]+", " ", str(text)).strip()
    if len(clean) > MAX_TEXT:
        clean = clean[: MAX_TEXT - 1] + "\u2026"
    longest = max((len(m) for m in re.findall(r"`+", clean)), default=0)
    fence = "`" * (longest + 1)
    pad = " " if clean.startswith("`") or clean.endswith("`") else ""
    return f"{fence}{pad}{clean or '-'}{pad}{fence}"


class Context:
    def __init__(self, run_gh: Gh, repo: str, dry_run: bool, server: str, out=print):
        self.gh, self.repo, self.dry_run, self.server, self.out = run_gh, repo, dry_run, server, out

    def write(self, what: str, args: list[str], payload: dict | None = None) -> None:
        """A write call. In a dry run it prints what it would do and does nothing."""
        if self.dry_run:
            self.out(f"DRY RUN would: {what}")
            if payload is not None:
                self.out(json.dumps(payload, indent=2, ensure_ascii=False))
            return
        self.out(f"{what}")
        self.gh(args, json.dumps(payload) if payload is not None else None)

    def url(self, path: str) -> str:
        return f"{self.server}/{self.repo}/{path}"


def failed_jobs(jobs: list[dict]) -> list[str]:
    return [j.get("name") or "(unnamed)" for j in jobs if j.get("conclusion") in FAILED_JOB]


def merged_pull(ctx: Context, sha: str) -> dict | None:
    """The pull request this commit merged, when it is a merge commit of one."""
    try:
        pulls = api_get(ctx.gh, f"repos/{ctx.repo}/commits/{sha}/pulls?per_page={PAGE}") or []
    except (GhError, ValueError) as exc:
        ctx.out(f"Pull request lookup failed, leaving it out: {exc}")
        return None
    for pull in pulls:
        if pull.get("merged_at") and pull.get("merge_commit_sha") == sha:
            return pull
    return None


def describe(ctx: Context, run: dict, jobs: list[dict]) -> list[str]:
    sha = run["head_sha"]
    lines = [
        f"- Run: [{run['id']}]({run['html_url']}) (attempt {run.get('run_attempt', 1)})",
        f"- Commit: [`{sha[:7]}`]({ctx.url('commit/' + sha)})",
        f"- Event: {code_span(run.get('event', ''))}",
    ]
    names = failed_jobs(jobs)
    if names:
        lines.append(f"- Failed jobs ({len(names)}):")
        lines += [f"  - {code_span(n)}" for n in names[:MAX_JOBS_LISTED]]
        if len(names) > MAX_JOBS_LISTED:
            lines.append(f"  - and {len(names) - MAX_JOBS_LISTED} more")
    else:
        lines.append("- Failed jobs: none reported")
    return lines


def issue_body(ctx: Context, run: dict, jobs: list[dict]) -> str:
    lines = ["CI failed on main. This issue stays open until a CI run on main is green again.", ""]
    lines += describe(ctx, run, jobs)
    pull = merged_pull(ctx, run["head_sha"])
    if pull:
        lines.append(f"- Merged pull request: #{int(pull['number'])} {code_span(pull.get('title', ''))}")
    lines += ["", "The person who merged it fixes or reverts it."]
    return "\n".join(lines)


def mentions_run(text: str, run_id: int) -> bool:
    return re.search(rf"/actions/runs/{run_id}(?!\d)", text or "") is not None


def open_issues(ctx: Context) -> list[dict]:
    path = f"repos/{ctx.repo}/issues?state=open&labels={quote(LABEL)}"
    found = [i for i in pages(ctx.gh, path) if "pull_request" not in i]
    return sorted(found, key=lambda i: i["number"])


def ensure_label(ctx: Context) -> None:
    try:
        ctx.gh(["api", f"repos/{ctx.repo}/labels/{quote(LABEL)}"])
        return
    except GhError as exc:
        if "404" not in str(exc) and "Not Found" not in str(exc):
            raise
    payload = {"name": LABEL, "color": LABEL_COLOR, "description": LABEL_DESCRIPTION}
    try:
        ctx.write(f"create label {LABEL!r}", ["api", "-X", "POST", f"repos/{ctx.repo}/labels", "--input", "-"], payload)
    except GhError as exc:
        if "already_exists" not in str(exc):
            raise


def retry_pending(run: dict, jobs: list[dict]) -> bool:
    """True when ci-retry.yml will rerun this failed first attempt, so its result is not final."""
    return run.get("run_attempt", 1) == 1 and any(name in RETRIED_JOBS for name in failed_jobs(jobs))


def newer_red_run(ctx: Context, run: dict) -> dict | None:
    """A later CI run on main that finished red: an older run going green must not close the issue."""
    path = f"repos/{ctx.repo}/actions/runs?branch=main&status=completed&per_page={PAGE}"
    for other in pages(ctx.gh, path, "workflow_runs")[:PAGE]:
        if other.get("id") == run.get("id") or other.get("name") != run.get("name"):
            continue
        if other.get("event") in ("pull_request", "pull_request_target"):
            continue
        if other.get("created_at", "") > run.get("created_at", "") and other.get("conclusion") in RED:
            return other
    return None


def handle(ctx: Context, run_id: int) -> str:
    run = api_get(ctx.gh, f"repos/{ctx.repo}/actions/runs/{run_id}")
    conclusion = run.get("conclusion")
    ctx.out(
        f"Run {run_id}: branch={run.get('head_branch')} event={run.get('event')} "
        f"status={run.get('status')} conclusion={conclusion} attempt={run.get('run_attempt')}"
    )
    if run.get("head_branch") != "main" or run.get("event") in ("pull_request", "pull_request_target"):
        ctx.out("Not a run on main (or a pull request run): nothing to do.")
        return "ignored"
    if run.get("status") != "completed" or (conclusion not in RED and conclusion != "success"):
        ctx.out(f"Conclusion {conclusion!r} is not a failure or a success: nothing to do.")
        return "ignored"

    issues = open_issues(ctx)
    if conclusion == "success":
        if not issues:
            ctx.out("Green and no open ci: main red issue: nothing to do.")
            return "none"
        newer = newer_red_run(ctx, run)
        if newer:
            ctx.out(f"A newer run ({newer['id']}) on main is red: leaving the issue open.")
            return "stale-green"
        sha = run["head_sha"]
        body = f"Green again at [`{sha[:7]}`]({ctx.url('commit/' + sha)}) (run [{run_id}]({run['html_url']}))"
        for issue in issues:
            n = issue["number"]
            ctx.write(f"comment on #{n}", ["api", "-X", "POST", f"repos/{ctx.repo}/issues/{n}/comments", "--input", "-"], {"body": body})
            ctx.write(f"close #{n}", ["api", "-X", "PATCH", f"repos/{ctx.repo}/issues/{n}", "--input", "-"],
                      {"state": "closed", "state_reason": "completed"})
        return "closed"

    jobs = pages(ctx.gh, f"repos/{ctx.repo}/actions/runs/{run_id}/jobs?filter=latest", "jobs")
    if retry_pending(run, jobs):
        ctx.out("A macOS desktop job failed on the first attempt: ci-retry reruns it once, so this waits for that rerun.")
        return "waiting-for-retry"
    if not issues:
        ensure_label(ctx)
        payload = {"title": TITLE, "body": issue_body(ctx, run, jobs), "labels": [LABEL]}
        ctx.write(f"open issue {TITLE!r}", ["api", "-X", "POST", f"repos/{ctx.repo}/issues", "--input", "-"], payload)
        return "opened"

    issue = issues[0]
    n = issue["number"]
    comments = pages(ctx.gh, f"repos/{ctx.repo}/issues/{n}/comments")
    if mentions_run(issue.get("body") or "", run_id) or any(mentions_run(c.get("body") or "", run_id) for c in comments):
        ctx.out(f"Run {run_id} is already on #{n}: nothing to do.")
        return "duplicate"
    body = "Red again.\n\n" + "\n".join(describe(ctx, run, jobs))
    ctx.write(f"comment on #{n}", ["api", "-X", "POST", f"repos/{ctx.repo}/issues/{n}/comments", "--input", "-"], {"body": body})
    return "commented"


def main(argv: list[str] | None = None, run_gh: Gh = gh, out=print) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--run-id", required=True, help="id of a finished CI run")
    parser.add_argument("--dry-run", action="store_true", help="print writes instead of making them")
    parser.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY", ""), help="owner/name (default $GITHUB_REPOSITORY, else the gh default)")
    args = parser.parse_args(argv)
    if not re.fullmatch(r"[0-9]{1,20}", args.run_id):
        out(f"Invalid run id {args.run_id!r}: digits only.")
        return 2
    try:
        repo = args.repo or run_gh(["repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"]).strip()
        if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repo):
            out(f"Invalid repository {repo!r}.")
            return 2
        server = os.environ.get("GITHUB_SERVER_URL", "https://github.com").rstrip("/")
        ctx = Context(run_gh, repo, args.dry_run, server, out)
        result = handle(ctx, int(args.run_id))
    except (GhError, ValueError, KeyError) as exc:
        out(f"main_red failed: {exc}")
        return 1
    out(f"Result: {result}{' (dry run)' if args.dry_run else ''}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
