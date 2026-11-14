#!/usr/bin/env python3
"""Tests for the check that ci.yml produces every required check name exactly once."""

import unittest
from pathlib import Path

from check_ci_required_names import DEFAULT_PATH, REQUIRED, check, read_jobs

GOOD = """\
name: CI
on: push
jobs:
  changes:
    name: Changes
    runs-on: ubuntu-24.04
  rust-build:
    name: Rust build (${{ matrix.os }})
    if: needs.changes.outputs.rust == 'true'
  rust:
    name: Rust (macos-15)
    if: always()
  web:
    name: "Web"
    if: always()
"""
NAMES = ("Rust (macos-15)", "Web")


class ReadJobs(unittest.TestCase):
    def test_names_and_conditions_are_read_per_job(self):
        jobs = read_jobs(GOOD)
        self.assertEqual(jobs["rust"], {"name": "Rust (macos-15)", "if": "always()"})
        self.assertEqual(jobs["web"]["name"], "Web")
        self.assertEqual(jobs["changes"], {"name": "Changes"})

    def test_steps_and_other_top_level_keys_are_not_jobs(self):
        text = GOOD + "    steps:\n      - name: Rust (macos-15)\n"
        self.assertEqual(check(text, NAMES), [])


class Accepted(unittest.TestCase):
    def test_always_running_jobs_with_plain_names_pass(self):
        self.assertEqual(check(GOOD, NAMES), [])

    def test_the_real_workflow_passes(self):
        self.assertEqual(check(Path(DEFAULT_PATH).read_text(encoding="utf-8")), [])
        self.assertEqual(len(REQUIRED), len(set(REQUIRED)))


class Refused(unittest.TestCase):
    def test_a_missing_name_is_refused(self):
        problems = check(GOOD.replace("Rust (macos-15)", "Rust build"), NAMES)
        self.assertTrue(any("'Rust (macos-15)'" in p and "0 jobs" in p for p in problems), problems)

    def test_a_duplicate_name_is_refused(self):
        text = GOOD + "  again:\n    name: Web\n    if: always()\n"
        self.assertTrue(any("2 jobs" in p for p in check(text, NAMES)))

    def test_a_required_job_that_can_skip_is_refused(self):
        text = GOOD.replace("    name: Rust (macos-15)\n    if: always()", "    name: Rust (macos-15)\n    if: needs.changes.outputs.rust == 'true'")
        self.assertTrue(any("if: always()" in p for p in check(text, NAMES)))

    def test_a_required_job_without_a_condition_is_refused(self):
        text = GOOD.replace('    name: "Web"\n    if: always()\n', '    name: "Web"\n')
        self.assertTrue(any("if: always()" in p for p in check(text, NAMES)))

    def test_a_matrix_name_is_refused_for_a_required_check(self):
        text = GOOD.replace("name: Rust (macos-15)", "name: Rust (${{ matrix.os }})")
        problems = check(text, ("Rust (${{ matrix.os }})",))
        self.assertTrue(any("expression" in p for p in problems), problems)

    def test_a_matrix_name_that_expands_to_a_required_name_is_refused(self):
        text = GOOD.replace("Rust build (${{ matrix.os }})", "Rust (${{ matrix.os }})")
        self.assertTrue(any("can expand" in p for p in check(text, NAMES)))


if __name__ == "__main__":
    unittest.main()
