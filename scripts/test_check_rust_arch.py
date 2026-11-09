#!/usr/bin/env python3
"""Tests for the Rust architecture check that CI runs on every Rust change."""

import unittest

from check_rust_arch import MAX_LINES, compare, scan

BIG = "x\n" * (MAX_LINES + 1)
OK = "x\n" * MAX_LINES


class RustArch(unittest.TestCase):
    def test_new_long_file_fails_and_baselined_one_passes(self):
        files = {"crates/orbyn-core/src/a.rs": BIG, "crates/orbyn-core/src/b.rs": OK}
        current = scan(files)
        self.assertEqual(list(current["long_files"]), ["crates/orbyn-core/src/a.rs"])
        self.assertEqual(len(compare(current, {})[0]), 1)
        self.assertEqual(compare(current, current), ([], []))

    def test_growth_of_a_baselined_file_only_warns(self):
        baseline = {"long_files": {"crates/orbyn-core/src/a.rs": MAX_LINES + 1}}
        current = scan({"crates/orbyn-core/src/a.rs": BIG + "y\n"})
        failures, warnings = compare(current, baseline)
        self.assertEqual(failures, [])
        self.assertEqual(len(warnings), 1)

    def test_orbynd_git_spawn_fails(self):
        files = {"crates/orbynd/src/routes.rs": 'let c = Command::new("git");\n'}
        failures, _ = compare(scan(files), {})
        self.assertEqual(len(failures), 1)
        self.assertEqual(compare(scan(files), scan(files)), ([], []))

    def test_git_spawn_outside_orbynd_is_not_this_rule(self):
        files = {"crates/orbyn-core/src/git.rs": 'Command::new("git")\n'}
        self.assertEqual(compare(scan(files), {}), ([], []))

    def test_orbynd_store_reference_count_may_not_grow(self):
        one = {"crates/orbynd/src/s.rs": "use orbyn_core::store::Store;\n"}
        two = {"crates/orbynd/src/s.rs": "use orbyn_core::store::Store;\nuse crate::store::X;\n"}
        self.assertEqual(compare(scan(one), scan(one)), ([], []))
        self.assertEqual(len(compare(scan(two), scan(one))[0]), 1)
        self.assertEqual(len(compare(scan(one), {})[0]), 1)


if __name__ == "__main__":
    unittest.main()
