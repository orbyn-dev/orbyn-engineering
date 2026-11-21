#!/usr/bin/env python3
"""Tests for ci_timing.py with a fake gh."""

import json
import os
import tempfile
import unittest

import ci_timing

REPO = "o/r"


def job(name, created, started, done, labels=("ubuntu-24.04",), conclusion="success"):
    return {"name": name, "created_at": created, "started_at": started, "completed_at": done,
            "labels": list(labels), "conclusion": conclusion}


def run(start="2026-11-20T10:00:00Z", end="2026-11-20T10:07:00Z"):
    return {"id": 5, "run_started_at": start, "updated_at": end}


def fake(run_obj, jobs):
    def gh(args, stdin=None):
        path = args[1]
        if "/jobs" in path:
            page = int(path.rsplit("page=", 1)[1])
            return json.dumps({"jobs": jobs[(page - 1) * 100: page * 100]})
        return json.dumps(run_obj)
    return gh


def go(run_obj, jobs, gh=None, extra=()):
    out = []
    with tempfile.TemporaryDirectory() as d:
        path = os.path.join(d, "summary.md")
        code = ci_timing.main(["--run-id", "5", "--repo", REPO, *extra], run_gh=gh or fake(run_obj, jobs),
                              out=out.append, summary_path=path)
        text = ""
        if os.path.exists(path):
            with open(path) as handle:
                text = handle.read()
    return code, text, out


class Timing(unittest.TestCase):
    def test_table_has_six_slowest_with_queue_run_and_runner(self):
        jobs = [job(f"j{i}", "2026-11-20T10:00:00Z", "2026-11-20T10:00:10Z", f"2026-11-20T10:0{i}:10Z") for i in range(1, 9)]
        jobs.append(job("local", "2026-11-20T10:00:00Z", "2026-11-20T10:01:30Z", "2026-11-20T10:10:30Z", labels=("self-hosted", "orbyn-local")))
        code, text, out = go(run(), jobs)
        self.assertEqual(code, 0)
        self.assertIn("Wall time: **7m 00s**", text)
        rows = [l for l in text.splitlines() if l.startswith("| ") and "---" not in l and "Job" not in l]
        self.assertEqual(len(rows), 6)
        self.assertIn("| local | 1m 30s | 9m 00s | self-hosted (orbyn-local) | success |", rows[0])
        self.assertIn("| j8 | 0m 10s | 8m 00s | hosted | success |", text)
        self.assertNotIn("| j1 |", text)
        self.assertEqual(out, [])

    def test_over_budget_warns(self):
        code, text, out = go(run(end="2026-11-20T10:08:01Z"), [])
        self.assertEqual(code, 0)
        self.assertEqual(len(out), 1)
        self.assertTrue(out[0].startswith("::warning"))
        self.assertIn("8m 01s", out[0])

    def test_push_run_over_budget_does_not_warn(self):
        _, text, out = go(run(end="2026-11-20T10:30:00Z"), [], extra=("--event", "push"))
        self.assertEqual(out, [])
        self.assertIn("applies to pull requests", text)
        _, _, out = go(run(end="2026-11-20T10:30:00Z"), [], extra=("--event", "pull_request"))
        self.assertEqual(len(out), 1)

    def test_exactly_budget_and_under_do_not_warn(self):
        for end in ("2026-11-20T10:08:00Z", "2026-11-20T10:05:00Z"):
            _, _, out = go(run(end=end), [])
            self.assertEqual(out, [], end)

    def test_job_with_null_timestamps_is_skipped(self):
        jobs = [job("never", "2026-11-20T10:00:00Z", None, None, conclusion="skipped"),
                job("half", "2026-11-20T10:00:00Z", "2026-11-20T10:00:05Z", None),
                job("ok", "2026-11-20T10:00:00Z", "2026-11-20T10:00:05Z", "2026-11-20T10:01:05Z")]
        _, text, _ = go(run(), jobs)
        self.assertIn("| ok |", text)
        self.assertNotIn("never", text)
        self.assertNotIn("half", text)

    def test_job_names_cannot_break_the_table(self):
        jobs = [job("a | b `c` <x>\n@org/team", "2026-11-20T10:00:00Z", "2026-11-20T10:00:05Z", "2026-11-20T10:01:05Z")]
        _, text, _ = go(run(), jobs)
        row = [l for l in text.splitlines() if "@org/team" in l][0]
        self.assertEqual(row.count("|"), 6)
        self.assertNotIn("`", row)

    def test_more_than_100_jobs_are_read(self):
        jobs = [job(f"j{i}", "2026-11-20T10:00:00Z", "2026-11-20T10:00:01Z", "2026-11-20T10:00:02Z") for i in range(130)]
        jobs.append(job("late", "2026-11-20T10:00:00Z", "2026-11-20T10:00:01Z", "2026-11-20T10:05:00Z"))
        _, text, _ = go(run(), jobs)
        self.assertIn("| late |", text)

    def test_api_error_still_exits_zero_and_is_in_summary(self):
        def boom(args, stdin=None):
            raise ci_timing.GhError("gh api: HTTP 500")
        code, text, out = go(None, None, gh=boom)
        self.assertEqual(code, 0)
        self.assertIn("Could not build the timing report", text)
        self.assertIn("HTTP 500", text)

    def test_garbage_response_and_bad_arguments_exit_zero(self):
        code, text, _ = go(None, None, gh=lambda a, s=None: "not json")
        self.assertEqual(code, 0)
        self.assertIn("Could not build", text)
        self.assertEqual(ci_timing.main(["--bogus"], out=lambda s: None, summary_path=os.devnull), 0)
        self.assertEqual(ci_timing.main(["--run-id", "x; rm", "--repo", REPO], out=lambda s: None, summary_path=os.devnull), 0)

    def test_unwritable_summary_still_exits_zero(self):
        code = ci_timing.main(["--run-id", "5", "--repo", REPO], run_gh=fake(run(), []), out=lambda s: None,
                              summary_path="/nonexistent-dir/x/summary.md")
        self.assertEqual(code, 0)

    def test_missing_run_times_are_reported_not_fatal(self):
        code, text, out = go({"id": 5}, [])
        self.assertEqual(code, 0)
        self.assertIn("Wall time: unknown", text)


if __name__ == "__main__":
    unittest.main()
