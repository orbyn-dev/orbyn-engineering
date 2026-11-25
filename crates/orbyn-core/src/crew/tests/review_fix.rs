//! The review stages' hard edges (slice 4b review round): a reviewer cannot edit what it reads, a model's silence clears
//! nothing, the fix loop is capped and keyed on the file, a fix stays on its file, agreement needs another vendor and is not
//! gameable, and two checks of one finding write one Gate.

use orbyn_contracts::GateStage;
use serde_json::json;

use super::harness::{Turn, World, world_tiered};
use super::review_support::{
    arc, built, commit_all, crew, finding, fixed, fixed_file, repro_for, reviewed,
};

fn failing(id: &str, path: &str) -> serde_json::Value {
    json!({ "id": id, "kind": "command",
        "params": { "run": format!("git grep --no-index -q NEVERHERE -- {path}") }, "proves": "It holds." })
}

/// The review task of the one lane a test asks for. A review answer lists its lanes in start order, not in a
/// fixture's order, and a lane a repo rule adds must not become the lane the test waits on.
fn one_lane(mut lanes: Vec<String>) -> String {
    assert_eq!(
        lanes.len(),
        1,
        "the test asks for one lane, and no rule adds another: {lanes:?}"
    );
    lanes.pop().expect("the lane's review task")
}

/// The review status row of the finding with this id, found by id so a reordered findings list cannot stand in for it.
fn finding_row<'a>(status: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
    status["findings"]
        .as_array()
        .expect("the review status lists its findings")
        .iter()
        .find(|row| row["id"].as_str() == Some(id))
        .unwrap_or_else(|| panic!("the review status has the finding {id}"))
}

/// The review status row of the finding filed for this path, found by path so the row does not depend on its place.
fn finding_at<'a>(status: &'a serde_json::Value, path: &str) -> &'a serde_json::Value {
    status["findings"]
        .as_array()
        .expect("the review status lists its findings")
        .iter()
        .find(|row| row["path"].as_str() == Some(path))
        .unwrap_or_else(|| panic!("the review status has the finding for {path}"))
}

/// The lane row called `lane` in a review answer or status, found by name so a rule-added lane cannot stand in for it.
fn lane_row<'a>(answer: &'a serde_json::Value, lane: &str) -> &'a serde_json::Value {
    answer["lanes"]
        .as_array()
        .expect("the answer lists its lanes")
        .iter()
        .find(|row| row["lane"].as_str() == Some(lane))
        .unwrap_or_else(|| panic!("the answer has the {lane} lane"))
}

/// The one fix brief a subject has: one confirmed finding starts one fix, so a second brief is a bug in the product.
fn only_fix(w: &World, subject: &str) -> crate::store::CrewFix {
    let mut fixes = w.core.store.crew_fixes_of(subject).unwrap();
    assert_eq!(
        fixes.len(),
        1,
        "one confirmed finding starts one fix: {fixes:?}"
    );
    fixes.pop().expect("the fix brief")
}

#[tokio::test]
async fn a_fix_with_spaces_and_valid_punctuation_gets_a_core_byte_check() {
    const PATH: &str = "src/file with spaces @plus+.txt";
    let w = crew(arc(|n, root| match n {
        0 => {
            std::fs::write(root.join(PATH), "original\n").unwrap();
            built(root)
        }
        1 => reviewed(vec![finding(PATH, 1, "Wrong value.")]),
        2 => reviewed(vec![finding(PATH, 2, "The value is wrong.")]),
        3 => fixed_file(root, PATH),
        _ => reviewed(vec![]),
    }))
    .await;
    let subject = w.built_work().await;
    let lane = one_lane(w.review(&subject, &["correctness"]).await);
    w.until_state(&lane, "passed").await;
    let id = w.first_finding(&subject);
    let (pending, error) = w.lead("crew_check_finding", json!({ "finding": id })).await;
    assert!(!error, "{pending}");
    w.until_state(pending["second_review"]["task"].as_str().unwrap(), "passed")
        .await;
    w.until("a fix is started", |w| {
        !w.core.store.crew_fixes_of(&subject).unwrap().is_empty()
    })
    .await;
    let fix = only_fix(&w, &subject).fix_task_id;
    w.until_state(&fix, "passed").await;
    let key = w.core.store.crew_answer_key(&fix).unwrap().unwrap();
    let check = key
        .checks
        .iter()
        .find(|c| c.id.as_str() == "chk-fix-touched")
        .unwrap();
    assert!(check.command().is_none());
    let digest = check.params["note"].as_str().unwrap();
    assert!(
        w.sent_to(&w.worker(&fix))
            .iter()
            .all(|prompt| !prompt.contains(digest))
    );
    let evidence = w.core.store.crew_evidence(&fix).unwrap();
    assert!(
        evidence
            .iter()
            .any(|e| e.check == check.id && e.outcome == orbyn_contracts::CheckOutcome::Pass)
    );
    assert_eq!(
        w.status(&subject)["blocked"],
        true,
        "touching a file never clears agreement alone"
    );
}

/// A reviewer that edits the code it reviews fails its run, and its findings can then not be confirmed.
#[tokio::test]
async fn a_reviewer_that_edits_the_work_fails_and_its_findings_are_not_confirmable() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        _ => {
            std::fs::write(root.join("src/lib.txt"), "tampered\n").unwrap();
            reviewed(vec![finding("src/lib.txt", 1, "It is wrong.")])
        }
    }))
    .await;
    let subject = w.built_work().await;
    let lane = one_lane(w.review(&subject, &["correctness"]).await);
    w.until_state(&lane, "stopped").await;
    let id = w.first_finding(&subject);
    let (refused, error) = w
        .lead(
            "crew_check_finding",
            json!({ "finding": id, "repro": failing("r1", "src/lib.txt") }),
        )
        .await;
    assert!(
        error
            && refused["message"]
                .as_str()
                .unwrap()
                .contains("did not pass"),
        "{refused}"
    );
    assert_eq!(w.status(&subject)["blocked"], false);
}

/// Any other file a reviewer leaves in the checkout fails the run too, so it cannot pass notes to a later review.
#[tokio::test]
async fn a_reviewer_that_leaves_a_file_fails() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        _ => {
            std::fs::create_dir_all(root.join(".orbyn/review")).unwrap();
            std::fs::write(
                root.join(".orbyn/review/notes.txt"),
                "the finding is at line 5\n",
            )
            .unwrap();
            reviewed(vec![])
        }
    }))
    .await;
    let subject = w.built_work().await;
    let lane = one_lane(w.review(&subject, &["correctness"]).await);
    w.until_state(&lane, "stopped").await;
}

/// A confirmed finding is cleared by a fix that changes its file, not by a fix that does nothing or by a reviewer that
/// goes quiet.
#[tokio::test]
async fn a_confirmed_finding_stays_until_a_real_fix_passes() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        1 => reviewed(vec![finding("src/lib.txt", 5, "Overflow.")]),
        2 => reviewed(vec![finding("src/lib.txt", 6, "A second reading.")]),
        3..=5 => {
            // A worker could alter the legacy comparison copy in writable Git metadata
            // without changing the finding's file. The immutable digest must still fail.
            if let Ok(copies) = std::fs::read_dir(root.join(".git/orbyn-fix")) {
                for copy in copies.flatten() {
                    if copy.path().extension().is_some_and(|ext| ext == "orig") {
                        std::fs::write(copy.path(), "tampered comparison copy\n").unwrap();
                    }
                }
            }
            Turn::Report(super::harness::report("done"))
        }
        _ => reviewed(vec![]),
    }))
    .await;
    let subject = w.built_work().await;
    let lane = one_lane(w.review(&subject, &["correctness"]).await);
    w.until_state(&lane, "passed").await;
    let id = w.first_finding(&subject);
    let (pending, _) = w.lead("crew_check_finding", json!({ "finding": id })).await;
    w.until_state(pending["second_review"]["task"].as_str().unwrap(), "passed")
        .await;
    w.until("a fix is started", |w| {
        !w.core.store.crew_fixes_of(&subject).unwrap().is_empty()
    })
    .await;
    let fix = only_fix(&w, &subject).fix_task_id;
    // The fix changes nothing, so the check that the file changed fails every attempt and the fix stops.
    w.until("the no-op fix ends", |w| {
        matches!(
            w.task(&fix).state.as_str(),
            "passed" | "stopped" | "needs_person"
        )
    })
    .await;
    assert_eq!(
        w.task(&fix).state,
        "stopped",
        "a mutable comparison copy must not make a no-op fix pass"
    );
    assert_eq!(w.status(&subject)["blocked"], true);
    // The lead runs the lane again; the reviewer says nothing. A model's silence clears nothing.
    let again = w.review(&subject, &["correctness"]).await;
    w.until_state(&one_lane(again), "passed").await;
    let status = w.status(&subject);
    assert_eq!(status["blocked"], true, "{status}");
    assert_eq!(finding_row(&status, &id)["status"], "unreported");
    assert_eq!(finding_row(&status, &id)["blocks"], true);
}

/// At most three fix briefs a reviewed task, however many places are confirmed.
#[tokio::test]
async fn the_fix_briefs_are_capped_per_task() {
    let w = crew(arc(|n, root| match n {
        0 => {
            for f in ["a", "b", "c", "d"] {
                std::fs::write(root.join(format!("src/{f}.txt")), "x\n").unwrap();
            }
            built(root)
        }
        1 => reviewed(
            ["a", "b", "c", "d"]
                .iter()
                .map(|f| finding(&format!("src/{f}.txt"), 1, &format!("Problem in {f}.")))
                .collect(),
        ),
        _ => Turn::Hang,
    }))
    .await;
    let subject = w.built_work().await;
    let lane = one_lane(w.review(&subject, &["correctness"]).await);
    w.until_state(&lane, "passed").await;
    let status = w.status(&subject);
    let findings = status["findings"]
        .as_array()
        .expect("the review status lists its findings");
    assert_eq!(findings.len(), 4);
    let mut answers = Vec::new();
    for (n, row) in findings.iter().enumerate() {
        let id = row["id"].as_str().expect("a finding's id");
        let path = row["path"].as_str().expect("the finding's path");
        let (done, error) = w
            .lead(
                "crew_check_finding",
                json!({ "finding": id, "repro": failing(&format!("r{n}"), path) }),
            )
            .await;
        assert!(!error, "{done}");
        answers.push(done["fix"].clone());
    }
    // The cap counts the requests, not the reviewer's fixture order: three briefs start and the rest stop.
    assert_eq!(
        answers.iter().filter(|f| f["started"] == true).count(),
        3,
        "{answers:?}"
    );
    let stopped: Vec<_> = answers
        .iter()
        .filter_map(|f| f["stopped"].as_str())
        .collect();
    assert_eq!(stopped, ["fix_limit"], "{answers:?}");
    assert_eq!(w.core.store.crew_fixes_of(&subject).unwrap().len(), 3);
}

/// A fix is refused for a file the task could not write, a test file, or a file that is not there.
#[tokio::test]
async fn a_fix_for_the_wrong_file_is_refused() {
    let w = crew(arc(|n, root| match n {
        0 => {
            std::fs::create_dir_all(root.join("src/tests")).unwrap();
            std::fs::write(root.join("src/tests/a.txt"), "x\n").unwrap();
            built(root)
        }
        1 => reviewed(vec![
            finding("docs/readme.txt", 1, "Outside the lease."),
            finding("src/tests/a.txt", 1, "A test file."),
            finding("src/missing.txt", 1, "Not there."),
        ]),
        _ => Turn::Hang,
    }))
    .await;
    std::fs::create_dir_all(w.repo.join("docs")).unwrap();
    std::fs::write(w.repo.join("docs/readme.txt"), "x\n").unwrap();
    commit_all(&w.repo, "docs");
    let subject = w.built_work().await;
    let lane = one_lane(w.review(&subject, &["correctness"]).await);
    w.until_state(&lane, "passed").await;
    let ids: Vec<String> = w.status(&subject)["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["id"].as_str().unwrap().to_owned())
        .collect();
    for (n, id) in ids.iter().enumerate() {
        let (done, error) = w
            .lead(
                "crew_check_finding",
                json!({ "finding": id, "repro": failing(&format!("r{n}"), "src/lib.txt") }),
            )
            .await;
        assert!(!error, "{done}");
        assert_eq!(done["fix"]["stopped"], "refused_path", "{done}");
    }
    assert!(w.core.store.crew_fixes_of(&subject).unwrap().is_empty());
}

/// A confirmed finding outside the reviewed task's own `file_scope` checks sends no fix: the fix inherits those checks and
/// would fail them after a wasted run. The lead is told a person must authorise a new task (#451).
#[tokio::test]
async fn a_fix_outside_the_tasks_scope_checks_is_refused() {
    let key = json!({
        "checks": [{
            "id": "chk-scope", "kind": "file_scope", "params": { "only": ["src/lib.txt"] },
            "proves": "Only the function's file changed."
        }],
        "protected_paths": []
    });
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        1 => reviewed(vec![finding("src/base.txt", 1, "The base file is wrong.")]),
        _ => Turn::Hang,
    }))
    .await;
    let subject = w.built_with(key).await;
    let lane = one_lane(w.review(&subject, &["correctness"]).await);
    w.until_state(&lane, "passed").await;
    let id = w.first_finding(&subject);
    let (done, error) = w
        .lead(
            "crew_check_finding",
            json!({ "finding": id, "repro": failing("r-outside", "src/base.txt") }),
        )
        .await;
    assert!(!error, "{done}");
    assert_eq!(done["fix"]["started"], false, "{done}");
    assert_eq!(done["fix"]["stopped"], "refused_path", "{done}");
    assert!(
        done["fix"]["why"]
            .as_str()
            .is_some_and(|why| why.contains("scope")),
        "{done}"
    );
    assert!(
        w.core.store.crew_fixes_of(&subject).unwrap().is_empty(),
        "no fix brief names a file the reviewed task's own checks do not allow"
    );
}

/// With one vendor configured the independent second review is refused with a stored reason, never run on the same vendor.
#[tokio::test]
async fn the_second_review_needs_another_vendor() {
    let w = world_tiered(
        true,
        arc(|n, root| match n {
            0 => built(root),
            _ => reviewed(vec![finding("src/lib.txt", 5, "Overflow.")]),
        }),
        &[("alpha", false)],
        true,
    )
    .await;
    let subject = w.built_work().await;
    let (waiting, error) = w
        .lead(
            "crew_review",
            json!({"task":subject,"lanes":["correctness"]}),
        )
        .await;
    assert!(!error, "{waiting}");
    let request = lane_row(&waiting, "correctness")["same_engine_consent"]["id"]
        .as_str()
        .unwrap();
    let lane = w
        .core
        .crew_review_consent(&subject, &json!({"request":request,"choice":"once"}))
        .await
        .unwrap();
    w.until_state(lane["task"].as_str().unwrap(), "passed")
        .await;
    let id = w.first_finding(&subject);
    let (done, error) = w.lead("crew_check_finding", json!({ "finding": id })).await;
    assert!(!error, "{done}");
    let second = &done["second_review"];
    assert_eq!(second["state"], "needs_person", "{done}");
    assert_eq!(second["routed"], false);
    assert!(
        second["refusal"]["rejected"]
            .to_string()
            .contains("another vendor"),
        "{done}"
    );
    let task = second["task"].as_str().unwrap();
    assert!(
        !w.core.store.crew_results(task).unwrap().is_empty(),
        "the reason is stored"
    );
    assert!(
        w.core
            .store
            .crew_gates(&subject)
            .unwrap()
            .iter()
            .all(|g| g.stage != GateStage::FindingCheck)
    );
    // The refused run is not running: the finding is not pending, and a repro can still decide it.
    assert_eq!(finding_row(&w.status(&subject), &id)["check"], "unchecked");
    let pass = json!({ "id": "r-pass", "kind": "command", "params": { "run": "exit 0" }, "proves": "It does not hold." });
    let (decided, error) = w
        .lead(
            "crew_check_finding",
            json!({ "finding": id, "repro": pass }),
        )
        .await;
    assert!(!error && decided["decision"] == "unconfirmed", "{decided}");
}

/// A second reviewer that files many findings in the file confirms nothing.
#[tokio::test]
async fn a_noisy_second_review_does_not_confirm() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        1 => reviewed(vec![finding("src/lib.txt", 5, "Overflow.")]),
        2 => reviewed(
            [6, 9, 12, 15]
                .iter()
                .map(|l| finding("src/lib.txt", *l, &format!("Reading {l}.")))
                .collect(),
        ),
        _ => Turn::Hang,
    }))
    .await;
    let subject = w.built_work().await;
    let lane = one_lane(w.review(&subject, &["correctness"]).await);
    w.until_state(&lane, "passed").await;
    let id = w.first_finding(&subject);
    let (done, _) = w.lead("crew_check_finding", json!({ "finding": id })).await;
    w.until_state(done["second_review"]["task"].as_str().unwrap(), "passed")
        .await;
    w.until("the gate is written", |w| {
        w.core
            .store
            .crew_gates(&subject)
            .unwrap()
            .iter()
            .any(|g| g.stage == GateStage::FindingCheck)
    })
    .await;
    let gate = w
        .core
        .store
        .crew_gates(&subject)
        .unwrap()
        .into_iter()
        .rev()
        .find(|g| g.stage == GateStage::FindingCheck)
        .unwrap();
    assert_eq!(gate.decision, orbyn_contracts::GateDecision::Unconfirmed);
    assert!(gate.reason.as_str().contains("too many"), "{}", gate.reason);
    assert!(w.core.store.crew_fixes_of(&subject).unwrap().is_empty());
}

/// Two checks of one finding at once write one Gate, and a fix at most once.
#[tokio::test]
async fn two_checks_at_once_write_one_gate() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        1 => reviewed(vec![finding("src/lib.txt", 1, "It is wrong.")]),
        _ => Turn::Hang,
    }))
    .await;
    let subject = w.built_work().await;
    let lane = one_lane(w.review(&subject, &["correctness"]).await);
    w.until_state(&lane, "passed").await;
    let id = w.first_finding(&subject);
    let pass = json!({ "id": "r-pass", "kind": "command", "params": { "run": "exit 0" }, "proves": "It does not hold." });
    let (a, b) = tokio::join!(
        w.lead(
            "crew_check_finding",
            json!({ "finding": id, "repro": failing("r-fail", "src/lib.txt") })
        ),
        w.lead(
            "crew_check_finding",
            json!({ "finding": id, "repro": pass })
        ),
    );
    assert!(a.1 != b.1, "exactly one check decides: {a:?} {b:?}");
    let gates = w.core.store.crew_gates(&subject).unwrap();
    assert_eq!(
        gates
            .iter()
            .filter(|g| g.stage == GateStage::FindingCheck)
            .count(),
        1
    );
    assert!(w.core.store.crew_fixes_of(&subject).unwrap().len() <= 1);
    // The check that lost wrote no Evidence: the task's three checks and the winner's one.
    assert_eq!(w.core.store.crew_evidence(&subject).unwrap().len(), 4);
}

/// The work already changed in the checkout, bar the finding's file, is held by its bytes during a fix.
#[tokio::test]
async fn a_fix_cannot_edit_the_rest_of_the_work() {
    let w = crew(arc(|n, root| match n {
        0 => {
            std::fs::write(root.join("src/other.txt"), "other\n").unwrap();
            built(root)
        }
        1 => reviewed(vec![finding("src/lib.txt", 1, "It is wrong.")]),
        _ => {
            let _ = fixed_file(root, "src/other.txt");
            fixed(root)
        }
    }))
    .await;
    let subject = w.built_work().await;
    let lane = one_lane(w.review(&subject, &["correctness"]).await);
    w.until_state(&lane, "passed").await;
    let id = w.first_finding(&subject);
    let (done, _) = w
        .lead(
            "crew_check_finding",
            json!({ "finding": id, "repro": repro_for("r-fix", "src/lib.txt") }),
        )
        .await;
    let fix = done["fix"]["task"].as_str().unwrap().to_owned();
    w.until_state(&fix, "stopped").await;
    assert_eq!(w.status(&subject)["blocked"], true);
}

fn hide(root: &std::path::Path, flag: &str) {
    crate::testing::run_git(root, &["update-index", flag, "src/base.txt"]);
    std::fs::write(root.join("src/base.txt"), "edited in secret\n").unwrap();
}

/// An index flag that hides an edit from git does not hide it from the core.
#[tokio::test]
async fn an_index_flag_does_not_hide_a_reviewers_edit() {
    for flag in ["--assume-unchanged", "--skip-worktree"] {
        let w = crew(arc(move |n, root| match n {
            0 => built(root),
            _ => {
                hide(root, flag);
                reviewed(vec![])
            }
        }))
        .await;
        let subject = w.built_work().await;
        let lane = one_lane(w.review(&subject, &["correctness"]).await);
        w.until_state(&lane, "stopped").await;
    }
}

/// Pre-existing flags are refused too: attribution does not make hidden edits observable.
#[tokio::test]
async fn a_flag_that_was_there_at_the_start_refuses_admission() {
    for flag in ["--assume-unchanged", "--skip-worktree"] {
        let w = crew(arc(|n, root| match n {
            0 => built(root),
            _ => reviewed(vec![]),
        }))
        .await;
        crate::testing::run_git(&w.repo, &["update-index", flag, "src/base.txt"]);
        let (answer, error) = w
            .start(super::harness::brief(json!({})), super::harness::key())
            .await;
        assert!(error, "{answer}");
        assert!(
            answer.to_string().contains("flags that hide changes"),
            "{answer}"
        );
    }
}

/// A finding confirmed by agreement has no repro. A cosmetic edit and a silent re-review do not clear it; the lead's repro
/// that passes after the fix does, and one that fails does not.
#[tokio::test]
async fn an_agreement_confirmed_finding_needs_a_repro_to_clear() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        1 => reviewed(vec![finding("src/lib.txt", 5, "Overflow.")]),
        2 => reviewed(vec![finding("src/lib.txt", 6, "A second reading.")]),
        3 => fixed(root),
        _ => reviewed(vec![]),
    }))
    .await;
    let subject = w.built_work().await;
    let lane = one_lane(w.review(&subject, &["correctness"]).await);
    w.until_state(&lane, "passed").await;
    let id = w.first_finding(&subject);
    // A repro cannot clear what no fix has answered.
    let (done, _) = w.lead("crew_check_finding", json!({ "finding": id })).await;
    w.until_state(done["second_review"]["task"].as_str().unwrap(), "passed")
        .await;
    w.until("a fix is started", |w| {
        !w.core.store.crew_fixes_of(&subject).unwrap().is_empty()
    })
    .await;
    let fix = only_fix(&w, &subject).fix_task_id;
    w.until_state(&fix, "passed").await;
    w.until("the lanes run again", |w| {
        // The round's own order is the contract: the re-review is the second round.
        lane_row(&w.status(&subject), "correctness")["rounds"]
            .as_array()
            .is_some_and(|r| r.len() == 2 && r[1]["state"] == "passed")
    })
    .await;
    // The fix passed, the file changed and the re-review is silent: the finding still blocks.
    let status = w.status(&subject);
    assert_eq!(status["blocked"], true, "{status}");
    assert_eq!(finding_row(&status, &id)["status"], "unreported");
    // A repro that still fails changes nothing.
    let (still, error) = w
        .lead(
            "crew_check_finding",
            json!({ "finding": id, "repro": failing("r-fail", "src/lib.txt") }),
        )
        .await;
    assert!(!error && still["decision"] == "still_failing", "{still}");
    assert_eq!(w.status(&subject)["blocked"], true);
    // The failed attempt is on record: Evidence cited by a failing `checks` Gate on the finding.
    let attempt = w
        .core
        .store
        .crew_gates(&subject)
        .unwrap()
        .into_iter()
        .find(|g| g.stage == GateStage::Checks && g.subject.as_str() == id)
        .expect("a gate for the attempt");
    assert_eq!(attempt.decision, orbyn_contracts::GateDecision::Fail);
    assert!(!attempt.evidence.is_empty());
    // One that passes after the fix clears it.
    let (cleared, error) = w
        .lead(
            "crew_check_finding",
            json!({ "finding": id, "repro": repro_for("r-pass", "src/lib.txt") }),
        )
        .await;
    assert!(!error && cleared["decision"] == "cleared", "{cleared}");
    let gates = w.core.store.crew_gates(&subject).unwrap();
    let pass = gates
        .iter()
        .rfind(|g| g.stage == GateStage::Checks && g.subject.as_str() == id)
        .unwrap();
    assert!(
        pass.reason
            .as_str()
            .contains("not shown to fail before the fix")
    );
    let status = w.status(&subject);
    assert_eq!(status["blocked"], false, "{status}");
    assert_eq!(finding_row(&status, &id)["status"], "fixed");
}

/// The lanes run again after the last running fix, not while another is still writing in the shared checkout.
#[tokio::test]
async fn the_lanes_wait_for_the_last_running_fix() {
    let w = crew(arc(|n, root| match n {
        0 => {
            std::fs::write(root.join("src/b.txt"), "b\n").unwrap();
            built(root)
        }
        1 => reviewed(vec![
            finding("src/lib.txt", 1, "A."),
            finding("src/b.txt", 1, "B."),
        ]),
        // The fix for b starts first and keeps running; the one for lib.txt passes.
        2 => Turn::Hang,
        3 => fixed(root),
        _ => reviewed(vec![]),
    }))
    .await;
    let subject = w.built_work().await;
    let lane = one_lane(w.review(&subject, &["correctness"]).await);
    w.until_state(&lane, "passed").await;
    let status = w.status(&subject);
    let b_finding = finding_at(&status, "src/b.txt")["id"]
        .as_str()
        .expect("the finding for src/b.txt");
    let a_finding = finding_at(&status, "src/lib.txt")["id"]
        .as_str()
        .expect("the finding for src/lib.txt");
    let (b, _) = w
        .lead(
            "crew_check_finding",
            json!({ "finding": b_finding, "repro": failing("r-b", "src/b.txt") }),
        )
        .await;
    assert_eq!(b["fix"]["started"], true, "{b}");
    let (a, _) = w
        .lead(
            "crew_check_finding",
            json!({ "finding": a_finding, "repro": repro_for("r-a", "src/lib.txt") }),
        )
        .await;
    let fix_a = a["fix"]["task"].as_str().unwrap().to_owned();
    w.until_state(&fix_a, "passed").await;
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    assert_eq!(
        w.core.store.crew_reviews_of(&subject).unwrap().len(),
        1,
        "no lane ran while a fix was running"
    );
}

/// A clean filter that reports the original bytes cannot hide a reviewer's edit: the core's git runs without the repository's filters.
#[cfg(unix)]
#[tokio::test]
async fn a_clean_filter_does_not_hide_a_reviewers_edit() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        _ => {
            crate::testing::run_git(root, &["config", "filter.hide.clean", "printf 'base\\n'"]);
            let info = root.join(".git/info");
            std::fs::create_dir_all(&info).unwrap();
            std::fs::write(info.join("attributes"), "src/base.txt filter=hide\n").unwrap();
            std::fs::write(root.join("src/base.txt"), "edited in secret\n").unwrap();
            reviewed(vec![])
        }
    }))
    .await;
    let subject = w.built_work().await;
    let lane = one_lane(w.review(&subject, &["correctness"]).await);
    w.until_state(&lane, "stopped").await;
}

/// A replace ref that makes the base commit read as another commit does not hide an edit either.
#[cfg(unix)]
#[tokio::test]
async fn a_replace_ref_does_not_hide_a_reviewers_edit() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        _ => {
            std::fs::write(root.join("src/base.txt"), "edited in secret\n").unwrap();
            let run = |args: &[&str], index: bool| {
                let mut command = std::process::Command::new("git");
                command.current_dir(root).args(args);
                if index {
                    command.env("GIT_INDEX_FILE", root.join(".git/scratch-index"));
                }
                let out = command.output().unwrap();
                String::from_utf8_lossy(&out.stdout).trim().to_owned()
            };
            run(&["add", "-A"], true);
            let tree = run(&["write-tree"], true);
            let base = run(&["rev-parse", "HEAD"], false);
            let fake = run(
                &[
                    "-c",
                    "user.name=t",
                    "-c",
                    "user.email=t@example.test",
                    "commit-tree",
                    &tree,
                    "-p",
                    &base,
                    "-m",
                    "x",
                ],
                false,
            );
            run(&["replace", "--force", &base, &fake], false);
            reviewed(vec![])
        }
    }))
    .await;
    let subject = w.built_work().await;
    let lane = one_lane(w.review(&subject, &["correctness"]).await);
    w.until_state(&lane, "stopped").await;
}

/// If git cannot list the index, nothing is passed: the run goes to a person.
#[tokio::test]
async fn a_check_that_cannot_be_made_goes_to_a_person() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        _ => {
            std::fs::write(root.join(".git/index"), "not an index").unwrap();
            reviewed(vec![])
        }
    }))
    .await;
    let subject = w.built_work().await;
    let lane = one_lane(w.review(&subject, &["correctness"]).await);
    w.until_state(&lane, "needs_person").await;
}
