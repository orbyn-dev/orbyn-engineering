//! Create a check (slice 4k, PR 4) on a real `Core` with a fake engine and a real temporary git repository: a proposal runs nothing, only
//! accept starts a worker, a worker may add test files only, the proof must fail on the escaped merge and pass on the fix (a check that
//! passes on both is refused), the `failed` state with retry and withdraw, a stale view, isolation between projects, and no lane removed.
//! The checks are `git grep` commands, so no test needs a POSIX shell, a platform restriction or a network. Every commit sets its own
//! user and email.

use std::sync::{Arc, Mutex};

use serde_json::json;

use super::harness::{Script, Turn, World, brief, key, report, world, world_confined};
use super::review_support::commit_all;
use crate::crew::escapes::check_paths;
use crate::error::CoreError;
use crate::testing::run_git;
use orbyn_contracts as wire;
use wire::{EscapeCheckState, EscapeEventKind, EscapeState};

const PATH: &str = "tests/escape_check.txt";
const RUN: &str = "git grep --no-index -q -f tests/escape_check.txt -- src/value.txt";

/// What the fake worker writes in its turns: the pattern the new test file holds, and whether it also touches other files.
#[derive(Clone)]
struct Plan {
    pattern: String,
    overreach: bool,
    rules: bool,
    hang: bool,
}

impl Plan {
    fn good() -> Self {
        Self {
            pattern: "fixed".into(),
            overreach: false,
            rules: false,
            hang: false,
        }
    }
}

struct Rig {
    w: World,
    plan: Arc<Mutex<Plan>>,
    project: String,
    escape: String,
    merge_task: String,
    escaped: String,
    fix: String,
}

fn git_out(repo: &std::path::Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .current_dir(repo)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.test"])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// A project whose merge escaped: the repository holds a base commit, the escaped merge (`src/value.txt` says "bug") and its fix
/// (it says "fixed"), recorded as a merge whose revert is the fix. A person has confirmed the escape.
async fn rig() -> Rig {
    rig_with(false).await
}

async fn rig_with(confine: bool) -> Rig {
    let plan = Arc::new(Mutex::new(Plan::good()));
    let script_plan = plan.clone();
    let script: Script = Arc::new(move |n, root| {
        if n == 0 {
            return Turn::Hang;
        }
        let plan = script_plan.lock().unwrap().clone();
        if plan.hang {
            return Turn::Hang;
        }
        std::fs::create_dir_all(root.join("tests")).unwrap();
        std::fs::write(root.join(PATH), format!("{}\n", plan.pattern)).unwrap();
        if plan.overreach {
            std::fs::write(root.join("src/extra.txt"), format!("turn {n}\n")).unwrap();
        }
        if plan.rules {
            std::fs::create_dir_all(root.join(".orbyn")).unwrap();
            std::fs::write(root.join(".orbyn/crew.json"), format!("{{\"turn\":{n}}}\n")).unwrap();
        }
        commit_all(root, "add a test");
        Turn::Report(report("added the test"))
    });
    let w = if confine {
        world_confined(script).await
    } else {
        world(true, script).await
    };
    let repo = w.repo.clone();
    std::fs::create_dir_all(repo.join(".orbyn")).unwrap();
    std::fs::write(repo.join(".orbyn/crew.json"), "{\"lanes\":[]}\n").unwrap();
    std::fs::write(repo.join("src/value.txt"), "value: bug\n").unwrap();
    commit_all(&repo, "the merge that escaped");
    let escaped = git_out(&repo, &["rev-parse", "HEAD"]);
    std::fs::write(repo.join("src/value.txt"), "value: fixed\n").unwrap();
    commit_all(&repo, "the fix");
    let fix = git_out(&repo, &["rev-parse", "HEAD"]);
    let branch = git_out(&repo, &["rev-parse", "--abbrev-ref", "HEAD"]);

    let project = w.core.store.thread(&w.lead).unwrap().project_id;
    let (answer, error) = w
        .start(
            brief(json!({ "files": [{ "path": "src/t1/**", "mode": "write" }], "outcome": "The merge." })),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
    let merge_task = answer["task"].as_str().unwrap().to_owned();
    let mut record =
        crate::crew::fixtures::merge_record(&merge_task, &repo.to_string_lossy(), &branch, "x");
    record.crew_id = w.task(&merge_task).crew_id;
    record.before_commit = git_out(&repo, &["rev-parse", &format!("{escaped}~1")]);
    record.merge_commit = escaped.clone();
    record.candidate = escaped.clone();
    record.engine = "alpha".into();
    w.core.store.add_crew_merge(&record).unwrap();
    w.core.store.set_merge_reverted(&merge_task, &fix).unwrap();
    let possible = w
        .core
        .store
        .crew_escape_of_merge(&merge_task)
        .unwrap()
        .unwrap();
    let escape = possible.id.clone();
    w.core
        .crew_escape_confirm(
            &escape,
            &wire::EscapeDecide {
                project: project.clone(),
                seen: wire::EscapeSeen {
                    state: EscapeState::Possible,
                    gap: None,
                },
                note: None,
            },
        )
        .unwrap();
    Rig {
        w,
        plan,
        project,
        escape,
        merge_task,
        escaped,
        fix,
    }
}

fn seen(check: Option<EscapeCheckState>) -> wire::EscapeCheckSeen {
    wire::EscapeCheckSeen {
        state: EscapeState::Confirmed,
        gap: None,
        check,
    }
}

impl Rig {
    fn propose_body(&self) -> wire::EscapeCheckPropose {
        wire::EscapeCheckPropose {
            project: self.project.clone(),
            seen: seen(None),
            path: PATH.into(),
            asserts: "The value is fixed.".into(),
            run: RUN.into(),
            fix_commit: None,
        }
    }

    fn act(&self, check: EscapeCheckState) -> wire::EscapeCheckAct {
        wire::EscapeCheckAct {
            project: self.project.clone(),
            seen: seen(Some(check)),
        }
    }

    async fn propose(&self) -> wire::EscapeCheckAnswer {
        self.w
            .core
            .crew_escape_check_propose(&self.escape, &self.propose_body())
            .await
            .unwrap()
    }

    async fn accept(&self) -> wire::EscapeCheckAnswer {
        self.w
            .core
            .crew_escape_check_accept(&self.escape, &self.act(EscapeCheckState::Proposed))
            .await
            .unwrap()
    }

    fn check(&self) -> crate::store::EscapeCheckRecord {
        self.w
            .core
            .store
            .crew_escape_check(&self.escape)
            .unwrap()
            .unwrap()
    }

    /// Waits until the check ends, approving the change when the proof waits for it (a machine that cannot restrict the proof, which
    /// is every test world but `world_confined`), and returns where it ended.
    async fn finish(&self) -> EscapeCheckState {
        let ended = |c: &crate::store::EscapeCheckRecord| {
            matches!(
                c.state,
                EscapeCheckState::Proven | EscapeCheckState::Failed | EscapeCheckState::Withdrawn
            )
        };
        self.w
            .until("the check ends or waits for approval", |_| {
                let c = self.check();
                ended(&c) || (c.state == EscapeCheckState::Writing && c.diff_commit.is_some())
            })
            .await;
        let waiting = self.check();
        if waiting.state == EscapeCheckState::Writing {
            self.approve(&waiting).await;
            self.w
                .until("the check ends after approval", |_| ended(&self.check()))
                .await;
        }
        self.check().state
    }

    async fn approve(&self, check: &crate::store::EscapeCheckRecord) -> wire::EscapeCheckAnswer {
        self.w
            .core
            .crew_escape_check_approve(
                &self.escape,
                &wire::EscapeCheckApprove {
                    project: self.project.clone(),
                    seen: seen(Some(EscapeCheckState::Writing)),
                    commit: check.diff_commit.clone().unwrap(),
                    diff_hash: check.diff_hash.clone().unwrap(),
                },
            )
            .await
            .unwrap()
    }

    fn tasks(&self) -> i64 {
        self.w
            .core
            .store
            .reader()
            .query_row("SELECT COUNT(*) FROM crew_tasks", [], |r| r.get(0))
            .unwrap()
    }

    fn kinds(&self) -> Vec<EscapeEventKind> {
        self.w
            .core
            .store
            .crew_escape_events(&self.escape, 100)
            .unwrap()
            .iter()
            .map(|e| e.kind)
            .collect()
    }

    fn audits(&self) -> Vec<String> {
        self.w
            .core
            .store
            .reader()
            .prepare("SELECT action FROM audit_events WHERE action LIKE 'crew.escape.check.%' ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    /// The fake worker works in the project's own checkout (the harness gives it no worktree of its own), so its commit stays there. A
    /// real worker commits in its own worktree and the project's branch is untouched; this puts the checkout back as that leaves it.
    fn reset(&self) {
        run_git(&self.w.repo, &["reset", "-q", "--hard", &self.fix]);
        run_git(&self.w.repo, &["clean", "-q", "-fd"]);
    }

    fn lanes(&self) -> (i64, String) {
        let rows = self
            .w
            .core
            .store
            .reader()
            .query_row("SELECT COUNT(*) FROM crew_review_controls", [], |r| {
                r.get(0)
            })
            .unwrap();
        let rules = std::fs::read_to_string(self.w.repo.join(".orbyn/crew.json")).unwrap();
        (rows, rules)
    }
}

#[tokio::test]
async fn a_proposal_is_data_and_starts_nothing() {
    let r = rig().await;
    let tasks = r.tasks();
    let prompts = r.w.prompts.lock().unwrap().len();
    let answer = r.propose().await;
    assert_eq!(answer.check.state, EscapeCheckState::Proposed);
    assert_eq!(answer.check.path, PATH);
    assert_eq!(answer.check.task_id, None);
    assert_eq!(answer.escape.state, EscapeState::Confirmed);
    assert!(
        answer
            .check
            .line
            .contains("Nothing runs until a person accepts")
    );
    // The fix is the revert the escape recorded, and the escaped commit is the merge's.
    assert_eq!(answer.check.fix_commit, r.fix);
    assert_eq!(r.tasks(), tasks, "no task");
    assert_eq!(
        r.w.prompts.lock().unwrap().len(),
        prompts,
        "no engine was asked anything"
    );
    assert_eq!(r.audits(), ["crew.escape.check.propose"]);
    assert_eq!(r.kinds().last(), Some(&EscapeEventKind::CheckProposed));
    // The detail read shows the check and is itself unchanged for an escape without one.
    let detail = r.w.core.crew_escape_detail(&r.escape).unwrap();
    assert_eq!(detail.check.unwrap().state, EscapeCheckState::Proposed);
}

#[tokio::test]
async fn a_proposal_is_refused_for_what_is_not_a_test_file_a_secret_or_an_unknown_commit() {
    let r = rig().await;
    let core = &r.w.core;
    for path in [
        "src/lib.rs",
        ".orbyn/crew.json",
        ".github/workflows/ci.yml",
        "tests/../src/lib.rs",
        "package.json",
        "tests/**",
    ] {
        let body = wire::EscapeCheckPropose {
            path: path.into(),
            ..r.propose_body()
        };
        let refused = core.crew_escape_check_propose(&r.escape, &body).await;
        assert!(
            matches!(refused, Err(CoreError::Invalid(_))),
            "{path}: {refused:?}"
        );
    }
    let secret = wire::EscapeCheckPropose {
        run: "curl -H 'Authorization: Bearer sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123456789' x"
            .into(),
        ..r.propose_body()
    };
    assert!(
        core.crew_escape_check_propose(&r.escape, &secret)
            .await
            .is_err()
    );
    let unknown = wire::EscapeCheckPropose {
        fix_commit: Some("1234567890abcdef1234567890abcdef12345678".into()),
        ..r.propose_body()
    };
    assert!(
        core.crew_escape_check_propose(&r.escape, &unknown)
            .await
            .is_err()
    );
    let same = wire::EscapeCheckPropose {
        fix_commit: Some(r.escaped.clone()),
        ..r.propose_body()
    };
    assert!(
        core.crew_escape_check_propose(&r.escape, &same)
            .await
            .is_err()
    );
    assert!(core.store.crew_escape_check(&r.escape).unwrap().is_none());
    assert!(r.audits().is_empty(), "a refused proposal writes nothing");
}

#[tokio::test]
async fn only_accept_starts_a_worker_and_a_proven_check_is_queued_like_any_change() {
    let r = rig().await;
    let lanes = r.lanes();
    r.propose().await;
    let tasks = r.tasks();
    assert_eq!(tasks, 1, "the merge's own task only");

    let accepted = r.accept().await;
    assert_eq!(accepted.check.state, EscapeCheckState::Writing);
    let task = accepted.check.task_id.clone().unwrap();
    assert_eq!(r.finish().await, EscapeCheckState::Proven);
    r.w.until("the queue answered", |_| r.check().note.is_some())
        .await;
    assert_eq!(r.tasks(), tasks + 1, "accept started exactly one task");
    assert_eq!(r.w.task(&task).state, "passed");

    let check = r.check();
    let proof = check.proof.clone().unwrap();
    assert_eq!(
        (
            proof.escaped_commit.as_str(),
            proof.fix_commit.as_str(),
            proof.fails_on_escape,
            proof.passes_on_fix
        ),
        (r.escaped.as_str(), r.fix.as_str(), true, true)
    );
    // The merge queue was asked through its own entry; this device has no required CI names set, so it said so and the check waits
    // for a person to queue it from the Review queue.
    let note = check.note.clone().unwrap();
    assert!(note.starts_with("Proven, not queued"), "{note}");
    assert!(note.contains("required CI"), "{note}");
    assert!(r.w.core.store.merge_entry(&task).unwrap().is_none());
    assert_eq!(
        r.kinds()
            .into_iter()
            .filter(|k| matches!(
                k,
                EscapeEventKind::CheckProposed | EscapeEventKind::CheckProven
            ))
            .collect::<Vec<_>>(),
        [EscapeEventKind::CheckProposed, EscapeEventKind::CheckProven]
    );
    // This world cannot restrict the proof, so a person approved the change it ran.
    assert_eq!(
        r.audits(),
        [
            "crew.escape.check.propose",
            "crew.escape.check.accept",
            "crew.escape.check.approve"
        ]
    );

    // The worker was told the file and the commits, never the key.
    let brief = r.w.core.store.crew_brief(&task).unwrap().unwrap();
    assert_eq!(brief.write_paths().collect::<Vec<_>>().len(), 1);
    let key = r.w.core.store.crew_answer_key(&task).unwrap().unwrap();
    assert!(
        key.protected_paths
            .iter()
            .any(|p| p.as_str() == crate::crew::RULES_FILE),
        "the rules file is protected"
    );
    let sent = r.w.sent_to(&r.w.worker(&task)).join("\n");
    assert!(sent.contains(PATH) && sent.contains(&r.escape), "{sent}");
    assert!(!sent.contains("chk-escape-scope"), "the key is never sent");
    assert!(!sent.contains(crate::crew::RULES_FILE));

    // Nothing was merged or removed: the project's rules and lanes are as they were, and the proof left no worktree behind.
    assert_eq!(r.lanes(), lanes, "no lane is removed");
    let scratch = r.w.core.config.data_dir.join("crew-escape-proof");
    assert!(
        std::fs::read_dir(&scratch).map_or(true, |mut d| d.next().is_none()),
        "the proof's worktrees are removed"
    );
    // A second accept is refused in words and starts nothing.
    let again =
        r.w.core
            .crew_escape_check_accept(&r.escape, &r.act(EscapeCheckState::Proven))
            .await;
    assert!(matches!(again, Err(CoreError::Invalid(_))), "{again:?}");
    assert_eq!(r.tasks(), tasks + 1);
}

#[tokio::test]
async fn a_check_that_passes_on_both_is_refused_then_retried_and_only_accept_starts_again() {
    let r = rig().await;
    r.plan.lock().unwrap().pattern = "value".into();
    r.propose().await;
    let first = r.accept().await.check.task_id.unwrap();
    assert_eq!(r.finish().await, EscapeCheckState::Failed);
    let failed = r.check();
    let proof = failed.proof.clone().unwrap();
    assert_eq!((proof.fails_on_escape, proof.passes_on_fix), (false, true));
    assert!(
        failed
            .note
            .unwrap()
            .contains("passes on the escaped merge too")
    );
    let row =
        r.w.core
            .crew_escape_detail(&r.escape)
            .unwrap()
            .check
            .unwrap();
    assert!(row.line.starts_with("Refused"), "{}", row.line);
    assert_eq!(r.kinds().last(), Some(&EscapeEventKind::CheckFailed));
    assert!(
        r.w.core.store.merge_entry(&first).unwrap().is_none(),
        "a refused check never reaches the merge queue"
    );

    // Retry puts it back to a proposal and starts nothing.
    r.reset();
    let tasks = r.tasks();
    let prompts = r.w.prompts.lock().unwrap().len();
    let answer =
        r.w.core
            .crew_escape_check_retry(&r.escape, &r.act(EscapeCheckState::Failed))
            .unwrap();
    assert_eq!(answer.check.state, EscapeCheckState::Proposed);
    assert_eq!(answer.check.task_id, None);
    assert_eq!(
        (r.tasks(), r.w.prompts.lock().unwrap().len()),
        (tasks, prompts)
    );

    // A person accepts again, with a worker that now writes the right test.
    r.plan.lock().unwrap().pattern = "fixed".into();
    let second =
        r.w.core
            .crew_escape_check_accept(&r.escape, &r.act(EscapeCheckState::Proposed))
            .await
            .unwrap()
            .check
            .task_id
            .unwrap();
    assert_ne!(first, second);
    assert_eq!(r.finish().await, EscapeCheckState::Proven);
    assert_eq!(r.check().attempts, 2);
    assert_eq!(r.tasks(), tasks + 1);
}

#[tokio::test]
async fn a_check_that_fails_on_the_fix_too_or_the_wrong_way_round_is_refused() {
    let r = rig().await;
    r.plan.lock().unwrap().pattern = "never-in-the-file".into();
    r.propose().await;
    r.accept().await;
    assert_eq!(r.finish().await, EscapeCheckState::Failed);
    let first = r.check();
    let proof = first
        .proof
        .clone()
        .unwrap_or_else(|| panic!("{:?}", first.note));
    assert_eq!((proof.fails_on_escape, proof.passes_on_fix), (true, false));
    assert!(first.note.unwrap().contains("fails on the fix too"));

    // The wrong way round: it passes on the escaped merge and fails on the fix.
    r.reset();
    r.w.core
        .crew_escape_check_retry(&r.escape, &r.act(EscapeCheckState::Failed))
        .unwrap();
    r.plan.lock().unwrap().pattern = "bug".into();
    r.accept().await;
    assert_eq!(r.finish().await, EscapeCheckState::Failed);
    let proof = r.check().proof.unwrap();
    assert_eq!((proof.fails_on_escape, proof.passes_on_fix), (false, false));
    assert!(r.check().note.unwrap().contains("wrong way round"));
}

#[tokio::test]
async fn a_worker_that_changes_anything_but_the_test_file_cannot_make_a_check() {
    let r = rig().await;
    let lanes = r.lanes();
    r.plan.lock().unwrap().overreach = true;
    r.propose().await;
    let task = r.accept().await.check.task_id.unwrap();
    assert_eq!(r.finish().await, EscapeCheckState::Failed);
    let check = r.check();
    assert_eq!(
        check.proof, None,
        "the proof never ran on a change that was not only a test file"
    );
    assert!(check.note.unwrap().contains("stopped"));
    assert_eq!(r.w.task(&task).state, "stopped");
    assert!(r.w.core.store.merge_entry(&task).unwrap().is_none());

    // The rules file is protected too: a worker that edits it fails its own verification and no check comes of it.
    r.reset();
    r.w.core
        .crew_escape_check_retry(&r.escape, &r.act(EscapeCheckState::Failed))
        .unwrap();
    {
        let mut plan = r.plan.lock().unwrap();
        plan.overreach = false;
        plan.rules = true;
    }
    let again = r.accept().await.check.task_id.unwrap();
    assert_eq!(r.finish().await, EscapeCheckState::Failed);
    assert_ne!(task, again);
    assert_eq!(r.check().proof, None);
    assert_eq!(r.lanes().0, lanes.0, "no lane is removed");
    r.reset();
    assert_eq!(
        r.lanes(),
        lanes,
        "and the repository's rules are as they were"
    );
}

#[tokio::test]
async fn the_committed_change_is_judged_against_the_test_globs_on_real_commits() {
    let r = rig().await;
    let repo = r.w.repo.clone();
    std::fs::create_dir_all(repo.join("tests")).unwrap();
    std::fs::write(repo.join("tests/old.txt"), "old\n").unwrap();
    commit_all(&repo, "an old test");
    let base = git_out(&repo, &["rev-parse", "HEAD"]);
    let change = |files: &[(&str, &str)], message: &str| {
        run_git(&repo, &["checkout", "-q", "--detach", &base]);
        for (path, body) in files {
            let target = repo.join(path);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(target, body).unwrap();
        }
        commit_all(&repo, message);
        git_out(&repo, &["rev-parse", "HEAD"])
    };
    let judge = |commit: &str| {
        let repo = repo.clone();
        let (base, commit) = (base.clone(), commit.to_owned());
        async move {
            crate::crew::escapes::escape_check_change(&repo, &base, &commit, PATH)
                .await
                .unwrap()
        }
    };

    let good = change(&[(PATH, "fixed\n")], "good");
    let files = judge(&good).await.unwrap();
    assert_eq!(files, vec![(PATH.to_owned(), b"fixed\n".to_vec())]);

    for (what, files) in [
        (
            "product source",
            vec![(PATH, "x\n"), ("src/lib.txt", "x\n")],
        ),
        (
            "the rules file",
            vec![(PATH, "x\n"), (".orbyn/crew.json", "{}\n")],
        ),
        (
            "a workflow",
            vec![(PATH, "x\n"), (".github/workflows/ci.yml", "x\n")],
        ),
        (
            "a manifest in a test folder",
            vec![(PATH, "x\n"), ("tests/package.json", "{}\n")],
        ),
        (
            "an existing test",
            vec![(PATH, "x\n"), ("tests/old.txt", "changed\n")],
        ),
        ("not the proposed file", vec![("tests/other.txt", "x\n")]),
    ] {
        let commit = change(&files, what);
        let refused = judge(&commit).await;
        assert!(refused.is_err(), "{what} must be refused");
    }
    // A second test file beside the proposed one is refused too: a check adds exactly the one file.
    let two = change(&[(PATH, "x\n"), ("tests/second.txt", "x\n")], "two");
    assert!(judge(&two).await.is_err());

    // A deletion is not an added file.
    run_git(&repo, &["checkout", "-q", "--detach", &base]);
    run_git(&repo, &["rm", "-q", "tests/old.txt"]);
    std::fs::create_dir_all(repo.join("tests")).unwrap();
    std::fs::write(repo.join(PATH), "x\n").unwrap();
    commit_all(&repo, "delete");
    let deleted = git_out(&repo, &["rev-parse", "HEAD"]);
    assert!(judge(&deleted).await.is_err());

    // No change at all is refused, and the rule itself is the one the module documents.
    assert!(judge(&base).await.is_err());
    assert!(check_paths::refusal(PATH).is_none());
}

#[cfg(unix)]
#[tokio::test]
async fn a_symlink_is_not_a_test_file() {
    let r = rig().await;
    let repo = r.w.repo.clone();
    let base = git_out(&repo, &["rev-parse", "HEAD"]);
    std::fs::create_dir_all(repo.join("tests")).unwrap();
    std::os::unix::fs::symlink("../src/value.txt", repo.join(PATH)).unwrap();
    commit_all(&repo, "a link");
    let link = git_out(&repo, &["rev-parse", "HEAD"]);
    let refused = crate::crew::escapes::escape_check_change(&repo, &base, &link, PATH)
        .await
        .unwrap();
    assert!(refused.unwrap_err().contains("link"));
}

#[tokio::test]
async fn withdraw_cancels_a_worker_still_writing_and_a_late_end_cannot_revive_it() {
    let r = rig().await;
    r.plan.lock().unwrap().hang = true;
    r.propose().await;
    let task = r.accept().await.check.task_id.unwrap();
    r.w.until("the worker runs", |w| w.task(&task).state == "running")
        .await;
    let answer =
        r.w.core
            .crew_escape_check_withdraw(&r.escape, &r.act(EscapeCheckState::Writing))
            .await
            .unwrap();
    assert_eq!(answer.check.state, EscapeCheckState::Withdrawn);
    r.w.until_state(&task, "cancelled").await;
    // The end of the cancelled worker reaches the check's driver and finds a withdrawn check: nothing changes.
    r.w.core.crew_escape_check_reconcile().await;
    assert_eq!(r.check().state, EscapeCheckState::Withdrawn);
    assert_eq!(r.check().proof, None);
    assert_eq!(r.kinds().last(), Some(&EscapeEventKind::CheckWithdrawn));
    assert_eq!(r.audits().last().unwrap(), "crew.escape.check.withdraw");
}

#[tokio::test]
async fn a_stale_view_an_invalid_transition_and_another_project_are_refused() {
    let r = rig().await;
    r.propose().await;
    let core = &r.w.core;

    // Accept with a view of a check that is not there is stale, and starts nothing.
    let tasks = r.tasks();
    let stale = core
        .crew_escape_check_accept(
            &r.escape,
            &wire::EscapeCheckAct {
                project: r.project.clone(),
                seen: seen(None),
            },
        )
        .await;
    assert!(matches!(stale, Err(CoreError::Stale(_))), "{stale:?}");
    assert_eq!(r.tasks(), tasks);
    assert_eq!(r.check().state, EscapeCheckState::Proposed);

    // A retry of a check that has not failed, and a propose over a live one, are refused in words.
    let retry = core.crew_escape_check_retry(&r.escape, &r.act(EscapeCheckState::Proposed));
    assert!(
        retry.is_ok(),
        "a repeat of a proposal is a no-op: {retry:?}"
    );
    let other_path = wire::EscapeCheckPropose {
        path: "tests/other.txt".into(),
        seen: seen(Some(EscapeCheckState::Proposed)),
        ..r.propose_body()
    };
    let over = core.crew_escape_check_propose(&r.escape, &other_path).await;
    assert!(matches!(over, Err(CoreError::Invalid(_))), "{over:?}");

    // Another project's escape is not found, for each of the four.
    let elsewhere = core.new_project("other", false, None).await.unwrap().id;
    let act = wire::EscapeCheckAct {
        project: elsewhere.clone(),
        seen: seen(Some(EscapeCheckState::Proposed)),
    };
    let body = wire::EscapeCheckPropose {
        project: elsewhere,
        ..r.propose_body()
    };
    assert!(matches!(
        core.crew_escape_check_propose(&r.escape, &body).await,
        Err(CoreError::NotFound(_))
    ));
    assert!(matches!(
        core.crew_escape_check_accept(&r.escape, &act).await,
        Err(CoreError::NotFound(_))
    ));
    assert!(matches!(
        core.crew_escape_check_retry(&r.escape, &act),
        Err(CoreError::NotFound(_))
    ));
    assert!(matches!(
        core.crew_escape_check_withdraw(&r.escape, &act).await,
        Err(CoreError::NotFound(_))
    ));
    assert_eq!(r.check().state, EscapeCheckState::Proposed);

    // A possible escape has no check, and a tooling gap has no proposal.
    core.crew_escape_set_gap(
        &r.escape,
        &wire::EscapeGapSet {
            project: r.project.clone(),
            seen: wire::EscapeSeen {
                state: EscapeState::Confirmed,
                gap: None,
            },
            gap: wire::EscapeGap::ToolingGap,
        },
    )
    .unwrap();
    core.crew_escape_check_withdraw(
        &r.escape,
        &wire::EscapeCheckAct {
            project: r.project.clone(),
            seen: wire::EscapeCheckSeen {
                state: EscapeState::Confirmed,
                gap: Some(wire::EscapeGap::ToolingGap),
                check: Some(EscapeCheckState::Proposed),
            },
        },
    )
    .await
    .unwrap();
    let tooling = core
        .crew_escape_check_propose(
            &r.escape,
            &wire::EscapeCheckPropose {
                seen: wire::EscapeCheckSeen {
                    state: EscapeState::Confirmed,
                    gap: Some(wire::EscapeGap::ToolingGap),
                    check: Some(EscapeCheckState::Withdrawn),
                },
                ..r.propose_body()
            },
        )
        .await;
    assert!(matches!(tooling, Err(CoreError::Invalid(why)) if why.contains("tooling gap")));
}

#[tokio::test]
async fn the_flag_off_hides_every_check_route_and_nothing_confirms_by_itself() {
    let r = rig().await;
    // Confirming is still only a person's: a proposal never changes the escape.
    r.propose().await;
    let escape = r.w.core.store.crew_escape(&r.escape).unwrap().unwrap();
    assert_eq!(escape.state, EscapeState::Confirmed);
    assert_eq!(r.merge_task, escape.merge_task_id);

    let off = super::harness::world(false, Arc::new(|_, _| Turn::Hang)).await;
    let body = wire::EscapeCheckPropose {
        project: off.core.store.thread(&off.lead).unwrap().project_id,
        seen: seen(None),
        path: PATH.into(),
        asserts: "x".into(),
        run: RUN.into(),
        fix_commit: None,
    };
    assert!(
        off.core
            .crew_escape_check_propose("esc_none", &body)
            .await
            .is_err()
    );
    let act = wire::EscapeCheckAct {
        project: body.project.clone(),
        seen: seen(None),
    };
    assert!(
        off.core
            .crew_escape_check_accept("esc_none", &act)
            .await
            .is_err()
    );
    assert!(off.core.crew_escape_check_retry("esc_none", &act).is_err());
    assert!(
        off.core
            .crew_escape_check_withdraw("esc_none", &act)
            .await
            .is_err()
    );
}

mod more;
