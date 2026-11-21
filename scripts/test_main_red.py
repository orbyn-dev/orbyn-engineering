#!/usr/bin/env python3
"""Tests for main_red.py with a fake gh: no network, no writes."""

import io
import json
import re
import unittest
from urllib.parse import unquote

import main_red
from main_red import GhError

REPO = "o/r"
SHA = "a" * 40


def run(conclusion="failure", rid=100, **kw):
    base = {"id": rid, "head_branch": "main", "head_sha": SHA, "event": "push", "status": "completed",
            "conclusion": conclusion, "html_url": f"https://github.com/o/r/actions/runs/{rid}", "run_attempt": 1,
            "name": "CI", "created_at": f"2026-11-20T10:{rid % 60:02d}:00Z"}
    base.update(kw)
    return base


class FakeGh:
    def __init__(self, runs, jobs=None, issues=None, labels=(main_red.LABEL,), pulls=None):
        self.runs = {r["id"]: r for r in runs}
        self.jobs = jobs or [{"name": "E2E shard 5/6", "conclusion": "failure"}, {"name": "Web", "conclusion": "success"}]
        self.issues = issues if issues is not None else []
        self.comments = {}
        self.labels = set(labels)
        self.pulls = pulls if pulls is not None else []
        self.writes = []
        self.job_pages = []

    def __call__(self, args, stdin=None):
        if args[0] == "repo":
            return REPO
        method = "GET"
        if args[1] == "-X":
            method, path = args[2], args[3]
        else:
            path = args[1]
        path, _, query = path.partition("?")
        q = dict(p.split("=", 1) for p in query.split("&") if "=" in p)
        page = int(q.get("page", 1))
        per = int(q.get("per_page", 100))
        data = json.loads(stdin) if stdin else None
        if method != "GET":
            self.writes.append((method, path, data))
        m = re.fullmatch(r"repos/o/r/actions/runs/(\d+)", path)
        if m:
            return json.dumps(self.runs[int(m.group(1))])
        if re.fullmatch(r"repos/o/r/actions/runs/\d+/jobs", path):
            self.job_pages.append(page)
            return json.dumps({"jobs": self.jobs[(page - 1) * per: page * per]})
        if path == "repos/o/r/actions/runs" and method == "GET":
            assert q["branch"] == "main" and q["status"] == "completed"
            listed = sorted(self.runs.values(), key=lambda r: r["created_at"], reverse=True)
            return json.dumps({"workflow_runs": listed[(page - 1) * per: page * per]})
        if path.startswith("repos/o/r/commits/") and path.endswith("/pulls"):
            return json.dumps(self.pulls)
        if path.startswith("repos/o/r/labels/"):
            if unquote(path.rsplit("/", 1)[1]) in self.labels:
                return "{}"
            raise GhError("gh api: Not Found (HTTP 404)")
        if path == "repos/o/r/labels" and method == "POST":
            self.labels.add(data["name"])
            return "{}"
        if path == "repos/o/r/issues" and method == "GET":
            assert unquote(q["labels"]) == main_red.LABEL
            open_ = [i for i in self.issues if i["state"] == "open"]
            return json.dumps(open_[(page - 1) * per: page * per])
        if path == "repos/o/r/issues" and method == "POST":
            issue = {"number": 7 + len(self.issues), "state": "open", "body": data["body"], "title": data["title"], "labels": data["labels"]}
            self.issues.append(issue)
            return json.dumps(issue)
        m = re.fullmatch(r"repos/o/r/issues/(\d+)/comments", path)
        if m:
            n = int(m.group(1))
            if method == "POST":
                self.comments.setdefault(n, []).append({"body": data["body"]})
                return "{}"
            return json.dumps(self.comments.get(n, []))
        m = re.fullmatch(r"repos/o/r/issues/(\d+)", path)
        if m and method == "PATCH":
            for i in self.issues:
                if i["number"] == int(m.group(1)):
                    i["state"] = data["state"]
            return "{}"
        raise AssertionError(f"unexpected gh call {args}")

    def go(self, rid, *flags):
        out = io.StringIO()
        code = main_red.main(["--run-id", str(rid), "--repo", REPO, *flags], run_gh=self, out=lambda s: out.write(s + "\n"))
        return code, out.getvalue()

    def count(self, method, suffix):
        return [w for w in self.writes if w[0] == method and w[1].endswith(suffix)]


class Guards(unittest.TestCase):
    MAC = [{"name": "Desktop build (macos-15)", "conclusion": "failure"}, {"name": "Web", "conclusion": "success"}]

    def test_first_attempt_with_a_retried_macos_job_waits_for_the_rerun(self):
        g = FakeGh([run()], jobs=self.MAC)
        code, out = g.go(100)
        self.assertEqual(code, 0)
        self.assertIn("Result: waiting-for-retry", out)
        self.assertEqual(g.writes, [])

    def test_retried_macos_failure_still_alerts_on_the_second_attempt(self):
        g = FakeGh([run(run_attempt=2)], jobs=self.MAC)
        g.go(100)
        self.assertEqual(len(g.count("POST", "/issues")), 1)

    def test_other_failures_alert_even_when_a_macos_job_did_not_fail(self):
        g = FakeGh([run()])
        g.go(100)
        self.assertEqual(len(g.count("POST", "/issues")), 1)

    def test_an_older_green_run_does_not_close_the_issue_when_a_newer_run_is_red(self):
        old_green = run("success", rid=100)
        new_red = run("failure", rid=105)
        g = FakeGh([old_green, new_red], issues=[{"number": 7, "state": "open", "body": "x", "title": "CI is red on main"}])
        code, out = g.go(100)
        self.assertEqual(code, 0)
        self.assertIn("Result: stale-green", out)
        self.assertEqual(g.writes, [])

    def test_a_newer_cancelled_or_pull_request_run_does_not_block_closing(self):
        g = FakeGh([run("success", rid=100), run("cancelled", rid=105), run("failure", rid=106, event="pull_request")],
                   issues=[{"number": 7, "state": "open", "body": "x", "title": "CI is red on main"}])
        _, out = g.go(100)
        self.assertIn("Result: closed", out)

    def test_the_newest_green_run_closes(self):
        g = FakeGh([run("failure", rid=99), run("success", rid=100)],
                   issues=[{"number": 7, "state": "open", "body": "x", "title": "CI is red on main"}])
        _, out = g.go(100)
        self.assertIn("Result: closed", out)


class Decisions(unittest.TestCase):
    def test_first_failure_opens_one_issue(self):
        g = FakeGh([run()], pulls=[{"number": 12, "title": "fix: thing", "merged_at": "x", "merge_commit_sha": SHA}])
        code, _ = g.go(100)
        self.assertEqual(code, 0)
        posts = g.count("POST", "/issues")
        self.assertEqual(len(posts), 1)
        issue = posts[0][2]
        self.assertEqual(issue["title"], "CI is red on main")
        self.assertEqual(issue["labels"], ["ci: main red"])
        body = issue["body"]
        for part in ("actions/runs/100", "[`aaaaaaa`](https://github.com/o/r/commit/" + SHA + ")", "push",
                     "`E2E shard 5/6`", "#12 `fix: thing`"):
            self.assertIn(part, body)
        self.assertNotIn("Web", body)

    def test_second_failing_run_comments_once_without_new_issue(self):
        g = FakeGh([run(rid=100), run(rid=101)])
        g.go(100)
        g.go(101)
        self.assertEqual(len(g.count("POST", "/issues")), 1)
        comments = g.count("POST", "/comments")
        self.assertEqual(len(comments), 1)
        self.assertIn("actions/runs/101", comments[0][2]["body"])

    def test_same_run_id_again_adds_nothing(self):
        g = FakeGh([run(rid=100), run(rid=101)])
        for rid in (100, 101, 101, 100):
            g.go(rid)
        self.assertEqual(len(g.count("POST", "/issues")), 1)
        self.assertEqual(len(g.count("POST", "/comments")), 1)

    def test_run_id_prefix_is_not_a_match(self):
        g = FakeGh([run(rid=100), run(rid=1000)])
        g.go(100)
        g.go(1000)
        self.assertEqual(len(g.count("POST", "/comments")), 1)

    def test_success_closes_with_comment(self):
        g = FakeGh([run(rid=100), run("success", rid=102)])
        g.go(100)
        g.go(102)
        body = g.count("POST", "/comments")[0][2]["body"]
        self.assertTrue(body.startswith("Green again at "))
        self.assertIn("actions/runs/102", body)
        self.assertEqual(g.issues[0]["state"], "closed")
        self.assertEqual(len(g.count("PATCH", "/7")), 1)

    def test_success_without_open_issue_does_nothing(self):
        g = FakeGh([run("success")])
        g.go(100)
        self.assertEqual(g.writes, [])

    def test_not_conclusive_runs_do_nothing(self):
        for conclusion in ("cancelled", "skipped", "neutral", None):
            g = FakeGh([run(conclusion, status="in_progress" if conclusion is None else "completed")])
            g.go(100)
            self.assertEqual(g.writes, [], conclusion)

    def test_failed_then_green_after_rerun_closes(self):
        g = FakeGh([run(rid=100)])
        g.go(100)
        g.runs[100] = run("success", rid=100, run_attempt=2)
        g.go(100)
        self.assertEqual(g.issues[0]["state"], "closed")

    def test_pull_request_and_other_branches_are_ignored(self):
        for kw in ({"event": "pull_request"}, {"head_branch": "feature"}):
            g = FakeGh([run(**kw)])
            g.go(100)
            self.assertEqual(g.writes, [], kw)

    def test_invalid_run_id_is_rejected(self):
        g = FakeGh([run()])
        self.assertEqual(g.go("1; rm -rf /")[0], 2)
        self.assertEqual(g.writes, [])


class Safety(unittest.TestCase):
    def test_dry_run_makes_no_write_calls(self):
        for conclusion in ("failure", "success"):
            g = FakeGh([run(conclusion)], issues=[{"number": 3, "state": "open", "body": "", "labels": []}], labels=())
            code, out = g.go(100, "--dry-run")
            self.assertEqual(code, 0)
            self.assertEqual(g.writes, [])
            self.assertIn("DRY RUN would", out)
        g = FakeGh([run()], labels=())
        _, out = g.go(100, "--dry-run")
        self.assertIn("would: create label", out)
        self.assertIn("would: open issue", out)

    def test_untrusted_title_is_inert_text(self):
        title = "$(rm -rf /); `id` @org/team @user #1 <!-- x --> [a](http://e)\n## fake"
        g = FakeGh([run()], jobs=[{"name": "bad @org/team `x`", "conclusion": "failure"}],
                   pulls=[{"number": 5, "title": title, "merged_at": "x", "merge_commit_sha": SHA}])
        g.go(100)
        body = g.count("POST", "/issues")[0][2]["body"]
        # Everything inside code spans removed: no @mention, no markup is left outside.
        outside = re.sub(r"(`+)(.+?)\1(?!`)", "", body, flags=re.S)
        for bad in ("@", "$(", "<!--", "[a]", "rm -rf"):
            self.assertNotIn(bad, outside)
        self.assertNotIn("\n## fake", body)
        self.assertEqual(len([l for l in body.splitlines() if l.startswith("## ")]), 0)

    def test_code_span_cannot_be_closed_by_backticks(self):
        span = main_red.code_span("a ``` b `")
        self.assertTrue(span.startswith("````"))
        self.assertTrue(span.endswith("````"))

    def test_bodies_travel_as_json_on_stdin(self):
        calls = []
        g = FakeGh([run()])
        def spy(args, stdin=None):
            calls.append((args, stdin))
            return g(args, stdin)
        main_red.main(["--run-id", "100", "--repo", REPO], run_gh=spy, out=lambda s: None)
        post = [c for c in calls if "POST" in c[0] and c[0][3] == "repos/o/r/issues"][0]
        self.assertEqual(post[0][-2:], ["--input", "-"])
        self.assertIn("CI is red on main", json.loads(post[1])["title"])

    def test_pull_lookup_failure_is_not_fatal(self):
        g = FakeGh([run()])
        orig = g.__call__
        def failing(args, stdin=None):
            if len(args) > 1 and args[1].endswith("/pulls") or "/pulls?" in " ".join(args):
                raise GhError("gh api: forbidden")
            return orig(args, stdin)
        out = io.StringIO()
        code = main_red.main(["--run-id", "100", "--repo", REPO], run_gh=failing, out=lambda s: out.write(s))
        self.assertEqual(code, 0)
        self.assertEqual(len(g.count("POST", "/issues")), 1)

    def test_api_error_exits_non_zero_without_writes(self):
        def boom(args, stdin=None):
            raise GhError("down")
        self.assertEqual(main_red.main(["--run-id", "1", "--repo", REPO], run_gh=boom, out=lambda s: None), 1)


class Scale(unittest.TestCase):
    def test_more_than_100_jobs_are_paginated(self):
        jobs = [{"name": f"job {i}", "conclusion": "failure" if i in (5, 150, 230) else "success"} for i in range(250)]
        g = FakeGh([run()], jobs=jobs)
        g.go(100)
        self.assertEqual(g.job_pages, [1, 2, 3])
        body = g.count("POST", "/issues")[0][2]["body"]
        for name in ("job 5", "job 150", "job 230"):
            self.assertIn(f"`{name}`", body)
        self.assertIn("Failed jobs (3)", body)

    def test_exactly_100_jobs_reads_next_page_then_stops(self):
        jobs = [{"name": f"j{i}", "conclusion": "success"} for i in range(100)]
        g = FakeGh([run()], jobs=jobs)
        g.go(100)
        self.assertEqual(g.job_pages, [1, 2])

    def test_long_failed_list_is_capped(self):
        jobs = [{"name": f"j{i}", "conclusion": "failure"} for i in range(120)]
        g = FakeGh([run()], jobs=jobs)
        g.go(100)
        body = g.count("POST", "/issues")[0][2]["body"]
        self.assertIn("and 70 more", body)

    def test_label_is_created_when_missing(self):
        g = FakeGh([run()], labels=())
        g.go(100)
        created = g.count("POST", "/labels")
        self.assertEqual(len(created), 1)
        self.assertEqual(created[0][2], {"name": "ci: main red", "color": "b60205",
                                         "description": "CI failed on main; one issue per incident"})
        self.assertIn("ci: main red", g.labels)
        g.go(100)
        self.assertEqual(len(g.count("POST", "/labels")), 1)


if __name__ == "__main__":
    unittest.main()
