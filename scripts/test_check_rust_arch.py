#!/usr/bin/env python3
"""Tests for the Rust architecture check that CI runs on every Rust change."""

import unittest

from check_rust_arch import MAX_LINES, compare, scan, stale_exceptions

BIG = "x\n" * (MAX_LINES + 1)
OK = "x\n" * MAX_LINES


class RustArch(unittest.TestCase):
    def test_new_long_file_fails_and_baselined_one_passes(self):
        files = {"crates/orbyn-core/src/a.rs": BIG, "crates/orbyn-core/src/b.rs": OK}
        current = scan(files)
        self.assertEqual(list(current["long_files"]), ["crates/orbyn-core/src/a.rs"])
        self.assertEqual(len(compare(current, {})[0]), 1)
        self.assertEqual(compare(current, current), ([], []))

    def test_growth_fails_and_shrink_only_notices(self):
        baseline = {"long_files": {"crates/orbyn-core/src/a.rs": MAX_LINES + 2}}
        grown = scan({"crates/orbyn-core/src/a.rs": BIG + "y\ny\ny\n"})
        self.assertEqual(len(compare(grown, baseline)[0]), 1)
        shrunk = scan({"crates/orbyn-core/src/a.rs": BIG})
        failures, notices = compare(shrunk, baseline)
        self.assertEqual((failures, len(notices)), ([], 1))

    def test_stale_baseline_entries_fail(self):
        baseline = {"long_files": {"crates/orbyn-core/src/a.rs": MAX_LINES + 1}, "orbynd_store_refs": {"crates/orbynd/src/s.rs": 1}}
        failures, _ = compare(scan({}), baseline)
        self.assertEqual(len(failures), 2)

    def test_grouped_use_and_comments(self):
        grouped = {"crates/orbynd/src/s.rs": "use orbyn_core::{Core, store::Store};\n"}
        self.assertEqual(scan(grouped)["orbynd_store_refs"], {"crates/orbynd/src/s.rs": 1})
        multi = {"crates/orbynd/src/s.rs": "use orbyn_core::{\n    Core,\n    store::Store,\n};\n"}
        self.assertEqual(scan(multi)["orbynd_store_refs"], {"crates/orbynd/src/s.rs": 1})
        commented = {"crates/orbynd/src/s.rs": '// use orbyn_core::store::Store;\n/* Command::new("git") */\n'}
        self.assertEqual(scan(commented)["orbynd_store_refs"], {})
        self.assertEqual(scan(commented)["orbynd_git_spawns"], {})

    def test_orbynd_git_spawn_fails(self):
        # Routed through child_env, so only the "no git spawn in the service" rule speaks.
        files = {"crates/orbynd/src/routes.rs": 'fn r() {\n    let c = Command::new("git");\n    child_env::for_tool(&mut c);\n}\n'}
        failures, _ = compare(scan(files), {})
        self.assertEqual(len(failures), 1)
        self.assertEqual(compare(scan(files), scan(files)), ([], []))

    def test_git_spawn_outside_orbynd_is_not_the_orbynd_rule(self):
        files = {"crates/orbyn-core/src/git.rs": 'fn run() {\n    let c = Command::new("git");\n    child_env::for_tool(&mut c);\n}\n'}
        self.assertEqual(compare(scan(files), {}), ([], []))

    # ---- spawns must go through host::child_env (account-threat-model control 2) ----

    def spawn_failures(self, files):
        return compare(scan(files), {})[0]

    def test_a_spawn_outside_a_routed_function_fails(self):
        bare = {"crates/orbyn-core/src/x.rs": 'fn run() {\n    let _ = Command::new("kill").status();\n}\n'}
        failures = self.spawn_failures(bare)
        self.assertEqual(len(failures), 1)
        self.assertIn("child_env", failures[0])
        for wrapper in ("std::process::Command::new", "tokio::process::Command::new", "CommandBuilder::new"):
            files = {"crates/orbyn-core/src/x.rs": f'fn run() {{\n    let c = {wrapper}("p");\n}}\n'}
            self.assertEqual(len(self.spawn_failures(files)), 1, wrapper)

    def test_a_spawn_in_a_function_that_routes_passes(self):
        for call in ("child_env::for_tool(&mut c);", "crate::host::child_env::for_engine(&mut c);", "let plan = child_env::for_terminal(true);"):
            files = {"crates/orbyn-core/src/x.rs": f'fn run() {{\n    let mut c = Command::new("p");\n    {call}\n}}\n'}
            self.assertEqual(self.spawn_failures(files), [], call)

    def test_routing_in_another_function_does_not_count(self):
        files = {"crates/orbyn-core/src/x.rs": 'fn a() {\n    child_env::for_tool(&mut c);\n}\n\nfn b() {\n    let _ = Command::new("kill").status();\n}\n'}
        self.assertEqual(len(self.spawn_failures(files)), 1)

    def test_a_method_in_an_impl_is_judged_by_its_own_body(self):
        routed = 'impl A {\n    fn kill(&self) {\n        let mut c = Command::new("p");\n        child_env::for_tool(&mut c);\n    }\n}\n'
        bare = 'impl A {\n    fn kill(&self) {\n        let _ = Command::new("p").status();\n    }\n}\n'
        self.assertEqual(self.spawn_failures({"crates/orbyn-core/src/x.rs": routed}), [])
        self.assertEqual(len(self.spawn_failures({"crates/orbyn-core/src/x.rs": bare})), 1)

    def test_test_code_and_comments_are_ignored(self):
        in_tests = 'fn ok() {}\n#[cfg(test)]\nmod tests {\n    fn t() {\n        let _ = Command::new("git");\n    }\n}\n'
        self.assertEqual(self.spawn_failures({"crates/orbyn-core/src/x.rs": in_tests}), [])
        gated = '#[cfg(all(test, unix))]\nmod tests {\n    fn t() {\n        let _ = Command::new("git");\n    }\n}\n'
        self.assertEqual(self.spawn_failures({"crates/orbyn-core/src/x.rs": gated}), [])
        commented = '// Command::new("kill")\n/* Command::new("kill") */\nfn a() {}\n'
        self.assertEqual(self.spawn_failures({"crates/orbyn-core/src/x.rs": commented}), [])
        for path in ("crates/orbyn-core/src/x/tests.rs", "crates/orbyn-core/src/x_tests.rs", "crates/orbyn-core/src/testing.rs", "crates/orbynd/tests/t.rs"):
            self.assertEqual(self.spawn_failures({path: 'fn t() {\n    let _ = Command::new("git");\n}\n'}), [], path)
        # Code after a test module is still code.
        after = '#[cfg(test)]\nmod tests {\n    fn t() {}\n}\n\nfn real() {\n    let _ = Command::new("kill").status();\n}\n'
        self.assertEqual(len(self.spawn_failures({"crates/orbyn-core/src/x.rs": after})), 1)

    def test_audited_exceptions_pass_more_fail_and_stale_ones_fail(self):
        from check_rust_arch import SPAWN_EXCEPTIONS

        def at(path, n):
            full = path if path.endswith(".rs") else path + "x.rs"
            return {full: "fn a() {\n" + '    let _ = Command::new("git");\n' * n + "}\n"}

        path, (count, _why) = next(iter(SPAWN_EXCEPTIONS.items()))
        # Exactly the audited number: nothing is said about this path.
        self.assertEqual(self.spawn_failures(at(path, count)), [])
        # One more than audited fails.
        more = [f for f in self.spawn_failures(at(path, count + 1)) if f.startswith(path)]
        self.assertEqual(len(more), 1)
        self.assertIn("unrouted spawns", more[0])
        # None left: the entry is stale and must be removed (judged on the whole tree, by main).
        stale = stale_exceptions(scan({}))
        self.assertEqual(len(stale), len(SPAWN_EXCEPTIONS))
        self.assertTrue(all("no longer has" in f for f in stale))
        self.assertEqual(stale_exceptions(scan(at(path, count))), [f for f in stale if not f.startswith(path)])

    def test_every_audited_exception_says_why(self):
        from check_rust_arch import SPAWN_EXCEPTIONS

        for path, (count, why) in SPAWN_EXCEPTIONS.items():
            self.assertGreater(count, 0, path)
            self.assertGreater(len(why), 20, path)


if __name__ == "__main__":
    unittest.main()
