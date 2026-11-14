//! Review stages on fake engines (slice 4b). `alpha` builds, `beta` reviews: a finding that no check confirms blocks
//! nothing; one a failing check confirms sends a fix brief, and the re-review shows it fixed; the same finding twice
//! stops the loop; a lane added after a pass runs; repo rules add lanes; a second review confirms by agreement.

use orbyn_contracts::{GateDecider, GateDecision, GateStage, RouteRole, RouteTier};
use serde_json::json;

use super::harness::{KEY_MARKER, Turn, bind};
use super::review_support::{arc, built, commit_all, crew, finding, fixed, repro, reviewed};

fn passing_repro() -> serde_json::Value {
    json!({ "id": "repro-ok", "kind": "command", "params": { "run": "exit 0" }, "proves": "The claim does not hold." })
}

/// Slice 4b, done when (1): an unconfirmed finding does not block.
#[tokio::test]
async fn an_unconfirmed_finding_does_not_block() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        _ => reviewed(vec![finding(
            "src/lib.txt",
            1,
            "It wraps on a large value.",
        )]),
    }))
    .await;
    let subject = w.built_work().await;
    let lanes = w.review(&subject, &["correctness"]).await;
    w.until_state(&lanes[0], "passed").await;

    // The lane is its own run, routed by role to the other vendor at the lead tier.
    let decision = w
        .core
        .store
        .crew_route_decisions(&lanes[0])
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(decision.role, RouteRole::Review);
    assert_eq!(decision.tier, RouteTier::Lead);
    assert_eq!(
        decision.lane.as_ref().map(|l| l.as_str()),
        Some("correctness")
    );
    assert_eq!(decision.engine.as_str(), "beta", "the builder was alpha");

    // The finding is typed and stamped by the core, and blocks nothing until a check confirms it.
    let status = w.status(&subject);
    assert_eq!(status["findings"].as_array().unwrap().len(), 1);
    assert_eq!(status["findings"][0]["check"], "unchecked");
    assert_eq!(status["blocked"], false);
    let id = w.first_finding(&subject);
    let stored = w.core.store.crew_finding(&id).unwrap().unwrap();
    assert_eq!(
        stored.finding.author.role,
        orbyn_contracts::CrewRole::Reviewer
    );
    assert_eq!(stored.finding.lane.as_str(), "correctness");

    // A repro that passes means the claim does not hold: unconfirmed, no fix brief, still not blocked.
    let (done, error) = w
        .lead(
            "crew_check_finding",
            json!({ "finding": id, "repro": passing_repro() }),
        )
        .await;
    assert!(!error, "{done}");
    assert_eq!(done["decision"], "unconfirmed");
    assert!(done["fix"].is_null());
    let gates = w.core.store.crew_gates(&subject).unwrap();
    let gate = gates
        .iter()
        .find(|g| g.stage == GateStage::FindingCheck)
        .unwrap();
    assert_eq!(gate.decision, GateDecision::Unconfirmed);
    assert_eq!(gate.decided_by, GateDecider::Code);
    assert!(
        !gate.evidence.is_empty(),
        "a gate decided by code cites evidence"
    );
    let status = w.status(&subject);
    assert_eq!(status["findings"][0]["check"], "unconfirmed");
    assert_eq!(status["blocked"], false);
    assert!(w.core.store.crew_fixes_of(&subject).unwrap().is_empty());
    assert_eq!(w.core.store.crew_tasks_in_crew(&w.lead).unwrap().len(), 2);

    // The finding check's Evidence is not the work's: a later lane is told the same count and reads the same commit.
    let later = w.review(&subject, &["security"]).await;
    w.until_state(&later[0], "passed").await;
    let prompts = w.sent_to(&w.worker(&later[0])).join("\n");
    assert!(
        prompts.contains("2 of 3 checks passed"),
        "the count changed"
    );
    let review = w.core.store.crew_review(&later[0]).unwrap().unwrap();
    assert_eq!(
        review.commit,
        w.core.store.crew_review(&lanes[0]).unwrap().unwrap().commit
    );

    // The reviewer was never shown the key, and its run is over (H7), so it can file nothing more.
    let thread = w.worker(&lanes[0]);
    for prompt in w.sent_to(&thread) {
        assert!(!prompt.contains(KEY_MARKER) && !prompt.contains("chk-run"));
    }
    let token = bind(&w.core, &thread, "run-late");
    let (late, error) = w
        .call(&token, "crew_finding", finding("src/lib.txt", 2, "Late."))
        .await;
    assert!(error && late["code"] == "run_ended", "{late}");
}

/// Slice 4b, done when (2) and (3): a confirmed finding sends a fix brief, and the re-review shows it fixed.
#[tokio::test]
async fn a_confirmed_finding_sends_a_fix_brief_and_the_re_review_shows_it_fixed() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        1 => reviewed(vec![finding("src/lib.txt", 1, "The fix file is missing.")]),
        2 => fixed(root),
        _ => reviewed(vec![]),
    }))
    .await;
    let subject = w.built_work().await;
    let lanes = w.review(&subject, &["correctness"]).await;
    w.until_state(&lanes[0], "passed").await;
    let id = w.first_finding(&subject);

    let (done, error) = w
        .lead(
            "crew_check_finding",
            json!({ "finding": id, "repro": repro("repro-fix") }),
        )
        .await;
    assert!(!error, "{done}");
    assert_eq!(done["decision"], "confirmed");
    assert_eq!(done["fix"]["started"], true, "{done}");
    let fix = done["fix"]["task"].as_str().unwrap().to_owned();
    assert_eq!(w.status(&subject)["blocked"], true);

    // The fix brief answers the finding, may write its file only, and is checked like crew_start's. Its key holds the
    // reviewed task's checks, the repro, a check that the file changed and a scope check; the builder is told none.
    let brief = w.core.store.crew_brief(&fix).unwrap().unwrap();
    assert_eq!(brief.refs.as_deref().map(<[_]>::len), Some(1));
    assert_eq!(brief.refs.unwrap()[0].as_str(), id);
    assert_eq!(brief.files.len(), 1);
    assert_eq!(brief.files[0].path.as_str(), "src/lib.txt");
    assert_eq!(
        w.core
            .store
            .crew_answer_key(&fix)
            .unwrap()
            .unwrap()
            .checks
            .len(),
        6
    );
    w.until_state(&fix, "passed").await;
    for prompt in w.sent_to(&w.worker(&fix)) {
        assert!(
            !prompt.contains(KEY_MARKER) && !prompt.contains("git grep"),
            "the key reached the builder"
        );
    }

    // The lanes run again on the fix, and the finding shows as fixed.
    w.until("the re-review finishes", |w| {
        let status = w.status(&subject);
        status["lanes"][0]["rounds"]
            .as_array()
            .is_some_and(|r| r.len() == 2 && r[1]["state"] == "passed")
    })
    .await;
    let status = w.status(&subject);
    assert_eq!(status["findings"][0]["status"], "fixed");
    assert_eq!(status["blocked"], false);
}

/// The same lane and file twice stops the loop, however the claim is worded: after a fix, a round that reports the place
/// again is open, and it is not sent to a builder a second time.
#[tokio::test]
async fn the_same_place_twice_stops_the_loop() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        2 => fixed(root),
        1 => reviewed(vec![finding("src/lib.txt", 1, "The fix file is missing.")]),
        // The same place, worded another way.
        _ => reviewed(vec![finding(
            "src/lib.txt",
            1,
            "Nothing about this is right yet.",
        )]),
    }))
    .await;
    let subject = w.built_work().await;
    let lanes = w.review(&subject, &["correctness"]).await;
    w.until_state(&lanes[0], "passed").await;
    let first = w.first_finding(&subject);
    let (done, _) = w
        .lead(
            "crew_check_finding",
            json!({ "finding": first, "repro": repro("repro-fix") }),
        )
        .await;
    assert_eq!(done["fix"]["started"], true, "{done}");
    w.until("the re-review reports it again", |w| {
        w.status(&subject)["findings"].as_array().unwrap().len() == 2
            && w.status(&subject)["lanes"][0]["rounds"][1]["state"] == "passed"
    })
    .await;
    let status = w.status(&subject);
    let again = &status["findings"][1];
    assert_eq!(again["status"], "open");
    assert_eq!(
        again["check"], "confirmed",
        "the confirmation of the same finding carries"
    );
    assert_eq!(again["blocks"], true);
    assert_eq!(again["loop_stopped"], true);
    assert_eq!(
        status["findings"][0]["status"], "unreported",
        "a reworded claim is not a fix"
    );

    // Confirming the new one sends no second fix brief.
    let (done, error) = w
        .lead(
            "crew_check_finding",
            json!({ "finding": again["id"], "repro": json!({
                "id": "repro-two", "kind": "command",
                "params": { "run": "git grep --no-index -q NEVERHERE -- src/lib.txt" }, "proves": "x" }) }),
        )
        .await;
    assert!(!error, "{done}");
    assert_eq!(done["fix"]["stopped"], "repeated_finding", "{done}");
    assert_eq!(w.core.store.crew_fixes_of(&subject).unwrap().len(), 1);
}

/// Slice 4b, done when (4): a lane added after a pass runs, and there is no limit on lanes.
#[tokio::test]
async fn a_lane_added_after_a_pass_runs() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        _ => reviewed(vec![]),
    }))
    .await;
    let subject = w.built_work().await;
    assert_eq!(w.task(&subject).state, "passed");
    let first = w.review(&subject, &["correctness"]).await;
    w.until_state(&first[0], "passed").await;
    // Later, with the task long final, more lanes at once.
    let more = w
        .review(
            &subject,
            &[
                "security",
                "performance",
                "accessibility",
                "design",
                "licence",
            ],
        )
        .await;
    assert_eq!(more.len(), 5);
    for task in &more {
        w.until_state(task, "passed").await;
    }
    assert_eq!(
        w.task(&subject).state,
        "passed",
        "a review never reopens the run (H7)"
    );
    let status = w.status(&subject);
    assert_eq!(status["lanes"].as_array().unwrap().len(), 6);
    // A lane named again is its next round.
    let again = w.review(&subject, &["security"]).await;
    w.until_state(&again[0], "passed").await;
    let review = w.core.store.crew_review(&again[0]).unwrap().unwrap();
    assert_eq!(review.round, 2);
}

fn commit_rules(w: &super::harness::World, text: &str) {
    std::fs::create_dir_all(w.repo.join(".orbyn")).unwrap();
    std::fs::write(w.repo.join(".orbyn/crew.json"), text).unwrap();
    commit_all(&w.repo, "rules");
}

/// Repo rules add lanes, and only add: a lane the change's paths ask for runs without being named. The rules are the
/// ones at the commit the work started from.
#[tokio::test]
async fn repo_rules_add_lanes() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        _ => reviewed(vec![]),
    }))
    .await;
    commit_rules(
        &w,
        &json!({ "required": ["checks"], "lanes": [
            { "lane": "security", "when": { "paths": ["src/**"] }, "locked": true },
            { "lane": "docs", "when": { "paths": ["docs/**"] } },
            { "lane": "licence" },
        ] })
        .to_string(),
    );
    let subject = w.built_work().await;
    let (answer, error) = w
        .lead(
            "crew_review",
            json!({ "task": subject, "lanes": ["correctness"] }),
        )
        .await;
    assert!(!error, "{answer}");
    let named: Vec<(&str, bool, bool)> = answer["lanes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            (
                l["lane"].as_str().unwrap(),
                l["by_rule"] == true,
                l["locked"] == true,
            )
        })
        .collect();
    assert_eq!(
        named,
        [
            ("correctness", false, false),
            ("security", true, true),
            ("licence", true, false)
        ],
        "docs is not asked for by a change under src/"
    );
}

/// A broken rule file at the base commit is an error, never an empty rule set. The file is valid JSON, so the build starts:
/// an unreadable file would block every engine first (see `classes`).
#[tokio::test]
async fn a_broken_rule_file_fails_closed() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        _ => reviewed(vec![]),
    }))
    .await;
    commit_rules(&w, r#"{"lanes":[{"lane":"Not A Token"}]}"#);
    let subject = w.built_work().await;
    let (broken, error) = w
        .lead(
            "crew_review",
            json!({ "task": subject, "lanes": ["correctness"] }),
        )
        .await;
    assert!(error && broken["code"] == "invalid_object", "{broken}");
}

/// The builder can edit the rule file in the checkout; the rule is the file at the commit the work started from.
#[tokio::test]
async fn a_worker_cannot_remove_a_rule_lane() {
    let w = crew(arc(|n, root| match n {
        0 => {
            std::fs::write(root.join(".orbyn/crew.json"), r#"{"lanes":[]}"#).unwrap();
            built(root)
        }
        _ => reviewed(vec![]),
    }))
    .await;
    commit_rules(&w, r#"{"lanes":[{"lane":"security","locked":true}]}"#);
    // A key with no scope check, so the edit itself does not fail the task.
    let key = json!({ "checks": [{ "id": "chk-run", "kind": "command",
        "params": { "run": "echo fine" }, "proves": "It runs." }], "protected_paths": [] });
    let subject = w.built_with(key).await;
    assert_eq!(
        std::fs::read_to_string(w.repo.join(".orbyn/crew.json")).unwrap(),
        r#"{"lanes":[]}"#
    );
    let (answer, error) = w
        .lead(
            "crew_review",
            json!({ "task": subject, "lanes": ["correctness"] }),
        )
        .await;
    assert!(!error, "{answer}");
    let lanes: Vec<&str> = answer["lanes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["lane"].as_str().unwrap())
        .collect();
    assert_eq!(
        lanes,
        ["correctness", "security"],
        "the lane the worker removed still runs"
    );
}

async fn second_review(line: u32) -> (super::harness::World, String, String) {
    let w = crew(arc(move |n, root| match n {
        0 => built(root),
        1 => reviewed(vec![finding("src/lib.txt", 5, "Overflow.")]),
        2 => reviewed(vec![finding("src/lib.txt", line, "A different reading.")]),
        3 => fixed(root),
        _ => reviewed(vec![]),
    }))
    .await;
    let subject = w.built_work().await;
    let lanes = w.review(&subject, &["correctness"]).await;
    w.until_state(&lanes[0], "passed").await;
    let id = w.first_finding(&subject);
    let (done, error) = w.lead("crew_check_finding", json!({ "finding": id })).await;
    assert!(!error, "{done}");
    assert_eq!(done["decision"], "pending");
    let second = done["second_review"]["task"].as_str().unwrap().to_owned();
    w.until_state(&second, "passed").await;
    w.until("the gate is written", |w| {
        w.core
            .store
            .crew_gates(&subject)
            .unwrap()
            .iter()
            .any(|g| g.stage == GateStage::FindingCheck)
    })
    .await;
    (w, subject, second)
}

/// A finding with no repro is confirmed only by an independent second review at the same place, on another vendor.
#[tokio::test]
async fn a_second_review_that_agrees_confirms_a_finding() {
    let (w, subject, second) = second_review(6).await;
    let gate = w.core.store.crew_gates(&subject).unwrap().pop().unwrap();
    assert_eq!(gate.decision, GateDecision::Confirmed);
    assert_eq!(gate.decided_by, GateDecider::Code);
    assert!(
        gate.reason
            .as_str()
            .contains("first reviewer on beta, second on alpha"),
        "{}",
        gate.reason
    );
    let evidence = w.core.store.crew_evidence(&subject).unwrap();
    assert!(
        evidence
            .iter()
            .any(|e| e.check.as_str() == "finding-agreement" && gate.evidence.contains(&e.id))
    );
    let first = w
        .core
        .store
        .crew_route_decisions(&w.core.store.crew_reviews_of(&subject).unwrap()[0].task_id)
        .unwrap()
        .pop()
        .unwrap();
    let again = w
        .core
        .store
        .crew_route_decisions(&second)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(again.role, RouteRole::FindingCheck);
    assert_ne!(
        again.engine, first.engine,
        "the second review is on the other vendor"
    );
    // The fix brief follows, and the second review was never shown the first finding.
    w.until("a fix is started", |w| {
        !w.core.store.crew_fixes_of(&subject).unwrap().is_empty()
    })
    .await;
    for prompt in w.sent_to(&w.worker(&second)) {
        assert!(
            !prompt.contains("Overflow"),
            "the second review saw the first claim"
        );
    }
}

#[tokio::test]
async fn a_second_review_elsewhere_leaves_the_finding_unconfirmed() {
    let (w, subject, _) = second_review(50).await;
    let gate = w.core.store.crew_gates(&subject).unwrap().pop().unwrap();
    assert_eq!(gate.decision, GateDecision::Unconfirmed);
    assert_eq!(w.status(&subject)["blocked"], false);
    assert!(w.core.store.crew_fixes_of(&subject).unwrap().is_empty());
}

/// A reviewer reads and reports: it posts only to the lead, writes no fact, starts nothing and files only its own findings.
#[tokio::test]
async fn a_reviewer_reads_and_reports() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        _ => Turn::Hang,
    }))
    .await;
    let subject = w.built_work().await;
    let lanes = w.review(&subject, &["correctness"]).await;
    let thread = w.worker(&lanes[0]);
    w.until("the reviewer runs", |w| {
        w.core.store.thread(&thread).unwrap().active_run.is_some()
    })
    .await;
    let token = bind(&w.core, &thread, "run-reviewer");
    let code = |v: &serde_json::Value| v["code"].as_str().unwrap_or("").to_owned();
    let (v, e) = w
        .call(
            &token,
            "crew_board_write",
            json!({ "key": "k", "class": "observation", "value": 1, "base_rev": 0 }),
        )
        .await;
    assert!(e && code(&v) == "not_authorized", "{v}");
    let (v, e) = w
        .call(
            &token,
            "crew_post",
            json!({ "to": "all", "kind": "fact", "body": "hi" }),
        )
        .await;
    assert!(e && code(&v) == "not_authorized", "{v}");
    let (v, e) = w
        .call(
            &token,
            "crew_post",
            json!({ "to": "lead", "kind": "fact", "body": "a note" }),
        )
        .await;
    assert!(!e, "{v}");
    let (v, e) = w
        .call(
            &token,
            "crew_start",
            json!({ "brief": {}, "answer_key": {} }),
        )
        .await;
    assert!(e && code(&v) == "depth_exceeded", "{v}");
    // The core stamps the lane and the commit; a draft that names them is refused.
    let mut forged = finding("src/lib.txt", 1, "x");
    forged["lane"] = json!("other");
    let (v, e) = w.call(&token, "crew_finding", forged).await;
    assert!(e && code(&v) == "invalid_object", "{v}");
    let (v, e) = w
        .call(
            &token,
            "crew_finding",
            finding("src/lib.txt", 1, "It overflows."),
        )
        .await;
    assert!(!e, "{v}");
    // The lead is not a reviewer, and a builder is not one either.
    let (v, e) = w.lead("crew_finding", finding("src/lib.txt", 1, "x")).await;
    assert!(e && code(&v) == "not_authorized", "{v}");
    let (v, e) = w
        .call(
            &bind(&w.core, &w.worker(&subject), "run-builder"),
            "crew_finding",
            finding("src/lib.txt", 1, "x"),
        )
        .await;
    assert!(e && code(&v) == "run_ended", "{v}");
}
