//! The review stages' hard edges (slice 4b review round): a reviewer cannot edit what it reads, a model's silence clears
//! nothing, the fix loop is capped and keyed on the file, a fix stays on its file, agreement needs another vendor and is not
//! gameable, and two checks of one finding write one Gate.

use orbyn_contracts::GateStage;
use serde_json::json;

use super::harness::{Turn, world_tiered};
use super::review_support::{
    arc, built, commit_all, crew, finding, fixed, fixed_file, repro_for, reviewed,
};

fn failing(id: &str, path: &str) -> serde_json::Value {
    json!({ "id": id, "kind": "command",
        "params": { "run": format!("git grep --no-index -q NEVERHERE -- {path}") }, "proves": "It holds." })
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
    let lanes = w.review(&subject, &["correctness"]).await;
    w.until_state(&lanes[0], "stopped").await;
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
    let lanes = w.review(&subject, &["correctness"]).await;
    w.until_state(&lanes[0], "stopped").await;
}

/// A confirmed finding is cleared by a fix that changes its file, not by a fix that does nothing or by a reviewer that
/// goes quiet.
#[tokio::test]
async fn a_confirmed_finding_stays_until_a_real_fix_passes() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        1 => reviewed(vec![finding("src/lib.txt", 5, "Overflow.")]),
        2 => reviewed(vec![finding("src/lib.txt", 6, "A second reading.")]),
        3..=5 => Turn::Report(super::harness::report("done")),
        _ => reviewed(vec![]),
    }))
    .await;
    let subject = w.built_work().await;
    let lanes = w.review(&subject, &["correctness"]).await;
    w.until_state(&lanes[0], "passed").await;
    let id = w.first_finding(&subject);
    let (pending, _) = w.lead("crew_check_finding", json!({ "finding": id })).await;
    w.until_state(pending["second_review"]["task"].as_str().unwrap(), "passed")
        .await;
    w.until("a fix is started", |w| {
        !w.core.store.crew_fixes_of(&subject).unwrap().is_empty()
    })
    .await;
    let fix = w.core.store.crew_fixes_of(&subject).unwrap()[0]
        .fix_task_id
        .clone();
    // The fix changes nothing, so the check that the file changed fails every attempt and the fix stops.
    w.until_state(&fix, "stopped").await;
    assert_eq!(w.status(&subject)["blocked"], true);
    // The lead runs the lane again; the reviewer says nothing. A model's silence clears nothing.
    let again = w.review(&subject, &["correctness"]).await;
    w.until_state(&again[0], "passed").await;
    let status = w.status(&subject);
    assert_eq!(status["blocked"], true, "{status}");
    assert_eq!(status["findings"][0]["status"], "unreported");
    assert_eq!(status["findings"][0]["blocks"], true);
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
    let lanes = w.review(&subject, &["correctness"]).await;
    w.until_state(&lanes[0], "passed").await;
    let status = w.status(&subject);
    let ids: Vec<String> = status["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(ids.len(), 4);
    let mut answers = Vec::new();
    for (n, id) in ids.iter().enumerate() {
        let path = format!("src/{}.txt", ["a", "b", "c", "d"][n]);
        let (done, error) = w
            .lead(
                "crew_check_finding",
                json!({ "finding": id, "repro": failing(&format!("r{n}"), &path) }),
            )
            .await;
        assert!(!error, "{done}");
        answers.push(done["fix"].clone());
    }
    assert!(
        answers[..3].iter().all(|f| f["started"] == true),
        "{answers:?}"
    );
    assert_eq!(answers[3]["stopped"], "fix_limit", "{answers:?}");
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
    let lanes = w.review(&subject, &["correctness"]).await;
    w.until_state(&lanes[0], "passed").await;
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
    let lanes = w.review(&subject, &["correctness"]).await;
    w.until_state(&lanes[0], "passed").await;
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
    let lanes = w.review(&subject, &["correctness"]).await;
    w.until_state(&lanes[0], "passed").await;
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
    let gate = w.core.store.crew_gates(&subject).unwrap().pop().unwrap();
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
    let lanes = w.review(&subject, &["correctness"]).await;
    w.until_state(&lanes[0], "passed").await;
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
    let lanes = w.review(&subject, &["correctness"]).await;
    w.until_state(&lanes[0], "passed").await;
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
        let lanes = w.review(&subject, &["correctness"]).await;
        w.until_state(&lanes[0], "stopped").await;
    }
}

/// A flag the checkout already had (a sparse checkout's, say) is not the reviewer's doing.
#[tokio::test]
async fn a_flag_that_was_there_at_the_start_is_not_a_failure() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        _ => reviewed(vec![]),
    }))
    .await;
    crate::testing::run_git(
        &w.repo,
        &["update-index", "--skip-worktree", "src/base.txt"],
    );
    let subject = w.built_work().await;
    let lanes = w.review(&subject, &["correctness"]).await;
    w.until_state(&lanes[0], "passed").await;
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
    let lanes = w.review(&subject, &["correctness"]).await;
    w.until_state(&lanes[0], "passed").await;
    let id = w.first_finding(&subject);
    // A repro cannot clear what no fix has answered.
    let (done, _) = w.lead("crew_check_finding", json!({ "finding": id })).await;
    w.until_state(done["second_review"]["task"].as_str().unwrap(), "passed")
        .await;
    w.until("a fix is started", |w| {
        !w.core.store.crew_fixes_of(&subject).unwrap().is_empty()
    })
    .await;
    let fix = w.core.store.crew_fixes_of(&subject).unwrap()[0]
        .fix_task_id
        .clone();
    w.until_state(&fix, "passed").await;
    w.until("the lanes run again", |w| {
        w.status(&subject)["lanes"][0]["rounds"]
            .as_array()
            .is_some_and(|r| r.len() == 2 && r[1]["state"] == "passed")
    })
    .await;
    // The fix passed, the file changed and the re-review is silent: the finding still blocks.
    let status = w.status(&subject);
    assert_eq!(status["blocked"], true, "{status}");
    assert_eq!(status["findings"][0]["status"], "unreported");
    // A repro that still fails changes nothing.
    let (still, error) = w
        .lead(
            "crew_check_finding",
            json!({ "finding": id, "repro": failing("r-fail", "src/lib.txt") }),
        )
        .await;
    assert!(!error && still["decision"] == "still_failing", "{still}");
    assert_eq!(w.status(&subject)["blocked"], true);
    // One that passes after the fix clears it.
    let (cleared, error) = w
        .lead(
            "crew_check_finding",
            json!({ "finding": id, "repro": repro_for("r-pass", "src/lib.txt") }),
        )
        .await;
    assert!(!error && cleared["decision"] == "cleared", "{cleared}");
    let status = w.status(&subject);
    assert_eq!(status["blocked"], false, "{status}");
    assert_eq!(status["findings"][0]["status"], "fixed");
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
    let lanes = w.review(&subject, &["correctness"]).await;
    w.until_state(&lanes[0], "passed").await;
    let ids: Vec<String> = w.status(&subject)["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["id"].as_str().unwrap().to_owned())
        .collect();
    let (b, _) = w
        .lead(
            "crew_check_finding",
            json!({ "finding": ids[1], "repro": failing("r-b", "src/b.txt") }),
        )
        .await;
    assert_eq!(b["fix"]["started"], true, "{b}");
    let (a, _) = w
        .lead(
            "crew_check_finding",
            json!({ "finding": ids[0], "repro": repro_for("r-a", "src/lib.txt") }),
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
