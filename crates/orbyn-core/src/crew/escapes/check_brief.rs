//! The brief and the key of a created check (docs/crew/escapes.md, "Create a check"). The brief is an ordinary crew task the core
//! writes on the lead's behalf and starts through the same function as `crew_start`, so it passes the same checks (shape, secrets, the
//! key's text, protected paths, the router). It carries the escape id, the one path the worker may write, what the test asserts and
//! how it is run, and the two commits it must separate. It carries no diff, no test body and no command output.
//!
//! The key is never sent to the worker. It holds one check, `file_scope` on the proposed path, so a worker that touches any other file
//! fails its own verification, and it protects the repository's rules file. The core then compares the committed change against the
//! test-file globs ([`super::check_paths`]) before any proof runs, so the key is a first line, never the only one.

use serde_json::{Value, json};

use crate::crew::RULES_FILE;
use crate::store::{EscapeCheckRecord, EscapeRecord};

/// The turns and minutes a check's worker gets.
const TURNS: u32 = 20;
const MINUTES: u32 = 30;

fn short(commit: &str) -> &str {
    &commit[..commit.len().min(12)]
}

/// The brief draft and the key draft for `check`, whose escaped merge is `escaped_commit`.
pub(super) fn drafts(
    escape: &EscapeRecord,
    check: &EscapeCheckRecord,
    escaped_commit: &str,
) -> (Value, Value) {
    let outcome = format!(
        "Escape {id}: a merge that got past review and checks was later found defective. Add ONE new test file at {path}. \
         The test must assert: {asserts} It is run with: {run} It must fail on commit {escaped} (the merge that escaped) \
         and pass on commit {fix} (the fix or revert). Add only test files: change, delete and rename nothing else, and \
         never touch a policy, CI or manifest file or the repository's rules file. Commit the new file, then report.",
        id = escape.id,
        path = check.path,
        asserts = check.asserts,
        run = check.run,
        escaped = short(escaped_commit),
        fix = short(&check.fix_commit),
    );
    let brief = json!({
        "kind": "implement",
        "outcome": outcome,
        "inputs": [],
        "files": [{ "path": check.path, "mode": "write" }],
        "allowed_effects": ["read_repo", "write_leased", "run_checks"],
        "stop_when": ["The new test file is committed."],
        "budget": { "turns": TURNS, "minutes": MINUTES },
    });
    let key = json!({
        "checks": [{
            "id": "chk-escape-scope",
            "kind": "file_scope",
            "params": { "only": [check.path] },
            "proves": "Nothing but the one new test file changed.",
        }],
        "protected_paths": [RULES_FILE],
    });
    (brief, key)
}
