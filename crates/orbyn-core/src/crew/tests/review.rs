//! Review stages on fake engines (slice 4b). `alpha` builds, `beta` reviews: a finding that no check confirms blocks
//! nothing; one a failing check confirms sends a fix brief, and the re-review shows it fixed; the same finding twice
//! stops the loop; a lane added after a pass runs; repo rules add lanes; a second review confirms by agreement.

use orbyn_contracts::{GateDecider, GateDecision, GateStage, RouteRole, RouteTier};
use serde_json::json;

use super::harness::{KEY_MARKER, Turn, bind};
use super::review_support::{
    arc, built, commit_all, crew, finding, fixed, lead_binding, repro, reviewed,
};

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

/// The status row of the lane called `lane`, found by name: the status orders its lanes by name, so an index is not
/// the lane the test asked for, and a lane a repo rule adds must not stand in for it.
fn lane_row<'a>(status: &'a serde_json::Value, lane: &str) -> &'a serde_json::Value {
    status["lanes"]
        .as_array()
        .expect("the review status lists its lanes")
        .iter()
        .find(|row| row["lane"].as_str() == Some(lane))
        .unwrap_or_else(|| panic!("the review status has the {lane} lane"))
}

/// The status row of the finding with this id, found by id so a reordered findings list cannot stand in for it.
fn finding_row<'a>(status: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
    status["findings"]
        .as_array()
        .expect("the review status lists its findings")
        .iter()
        .find(|row| row["id"].as_str() == Some(id))
        .unwrap_or_else(|| panic!("the review status has the finding {id}"))
}

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
    let lane = one_lane(w.review(&subject, &["correctness"]).await);
    w.until_state(&lane, "passed").await;

    // The lane is its own run, routed by role to the other vendor. `correctness` is the lane a medium or high risk merge is owed
    // and no merge entry says low yet, so it keeps the lead tier.
    let decision = w
        .core
        .store
        .crew_route_decisions(&lane)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(decision.role, RouteRole::Review);
    assert_eq!(decision.tier, RouteTier::Lead);
    // The fake engines list one model, assigned the lead tier by the harness, so the lane's model is that one.
    assert_eq!(decision.model.as_str(), "beta-model");
    assert_eq!(
        decision.lane.as_ref().map(|l| l.as_str()),
        Some("correctness")
    );
    assert_eq!(decision.engine.as_str(), "beta", "the builder was alpha");

    // The finding is typed and stamped by the core, and blocks nothing until a check confirms it.
    let status = w.status(&subject);
    assert_eq!(status["findings"].as_array().unwrap().len(), 1);
    let id = w.first_finding(&subject);
    assert_eq!(finding_row(&status, &id)["check"], "unchecked");
    assert_eq!(status["blocked"], false);
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
    assert_eq!(finding_row(&status, &id)["check"], "unconfirmed");
    assert_eq!(status["blocked"], false);
    assert!(w.core.store.crew_fixes_of(&subject).unwrap().is_empty());
    assert_eq!(w.core.store.crew_tasks_in_crew(&w.lead).unwrap().len(), 2);

    // The finding check's Evidence is not the work's: a later lane is told the same count and reads the same commit.
    let later = one_lane(w.review(&subject, &["security"]).await);
    w.until_state(&later, "passed").await;
    let prompts = w.sent_to(&w.worker(&later)).join("\n");
    assert!(
        prompts.contains("2 of 3 checks passed"),
        "the count changed"
    );
    let review = w.core.store.crew_review(&later).unwrap().unwrap();
    assert_eq!(
        review.commit,
        w.core.store.crew_review(&lane).unwrap().unwrap().commit
    );

    // The reviewer was never shown the key, and its run is over (H7), so it can file nothing more.
    let thread = w.worker(&lane);
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
    let lane = one_lane(w.review(&subject, &["correctness"]).await);
    w.until_state(&lane, "passed").await;
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
    let refs = brief
        .refs
        .as_deref()
        .expect("the brief answers the finding");
    assert_eq!(refs.len(), 1, "the brief answers one finding");
    assert_eq!(refs.first().expect("the finding id").as_str(), id);
    let files: Vec<&str> = brief.files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(
        files,
        ["src/lib.txt"],
        "the brief may write the finding's file only"
    );
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
        // The round's own order is the contract: the re-review is the second round.
        lane_row(&status, "correctness")["rounds"]
            .as_array()
            .is_some_and(|r| r.len() == 2 && r[1]["state"] == "passed")
    })
    .await;
    let status = w.status(&subject);
    assert_eq!(finding_row(&status, &id)["status"], "fixed");
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
    let lane = one_lane(w.review(&subject, &["correctness"]).await);
    w.until_state(&lane, "passed").await;
    let first = w.first_finding(&subject);
    let (done, _) = w
        .lead(
            "crew_check_finding",
            json!({ "finding": first, "repro": repro("repro-fix") }),
        )
        .await;
    assert_eq!(done["fix"]["started"], true, "{done}");
    w.until("the re-review reports it again", |w| {
        let status = w.status(&subject);
        // The round's own order is the contract: the re-review is the second round.
        status["findings"].as_array().unwrap().len() == 2
            && lane_row(&status, "correctness")["rounds"][1]["state"] == "passed"
    })
    .await;
    let status = w.status(&subject);
    // The re-review files the reworded claim at the same place, so its row is the one that is not the first finding.
    let again = status["findings"]
        .as_array()
        .expect("the review status lists its findings")
        .iter()
        .find(|row| row["id"].as_str() != Some(first.as_str()))
        .unwrap_or_else(|| panic!("the re-review's new finding"));
    assert_eq!(again["status"], "open");
    assert_eq!(
        again["check"], "confirmed",
        "the confirmation of the same finding carries"
    );
    assert_eq!(again["blocks"], true);
    assert_eq!(again["loop_stopped"], true);
    assert_eq!(
        finding_row(&status, &first)["status"],
        "unreported",
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
    let first = one_lane(w.review(&subject, &["correctness"]).await);
    w.until_state(&first, "passed").await;
    // Later, with the task long final, more lanes. Each call starts its lanes one after the other inside the tool's
    // 45 second budget, and a call that runs out is an error naming its lanes (#628); a lane takes seconds on a slow
    // Windows runner, so the five still come in short calls.
    let mut more = w.review(&subject, &["security", "performance"]).await;
    more.extend(w.review(&subject, &["accessibility", "design"]).await);
    more.extend(w.review(&subject, &["licence"]).await);
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
    let again = one_lane(w.review(&subject, &["security"]).await);
    w.until_state(&again, "passed").await;
    let review = w.core.store.crew_review(&again).unwrap().unwrap();
    assert_eq!(review.round, 2);
}

/// Issue #628: a review call cut by the tool budget is an error that names the lanes that started and the lanes
/// it did not reach, the lane it started keeps running, and a retry asks for only the missing ones.
#[tokio::test]
async fn a_budget_cut_review_call_names_its_lanes() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        _ => reviewed(vec![]),
    }))
    .await;
    let subject = w.built_work().await;
    // The deadline already reached: the first lane still starts, so a call makes progress on any machine, and the
    // loop answers before the MCP layer would drop it, naming what it started and what it did not reach.
    let binding = lead_binding(&w);
    let answer = w
        .core
        .crew_lanes_tool(
            &binding,
            &json!({ "task": subject, "lanes": ["correctness", "security", "performance"] }),
            std::time::Instant::now(),
            std::time::Duration::ZERO,
        )
        .await;
    let Err(cut) = answer else {
        panic!("a cut review call is an error, not {answer:?}");
    };
    assert_eq!(cut.code, orbyn_contracts::ErrorCode::InvalidObject, "{cut}");
    let one = w
        .core
        .store
        .crew_reviews_of(&subject)
        .unwrap()
        .into_iter()
        .find(|review| review.lane == "correctness")
        .expect("the cut call's first lane started");
    assert_eq!(
        cut.message,
        format!(
            "lanes: the tool budget ran out. Started: correctness ({}). Not reached: security, performance. Ask again with task {subject} and lanes [\"security\",\"performance\"]; lanes the repo's rules ask for are added on every call, so asking again re-runs them, even one that started above.",
            one.task_id
        )
    );
    w.until_state(&one.task_id, "passed").await;
    assert!(
        w.core
            .store
            .crew_reviews_of(&subject)
            .unwrap()
            .iter()
            .all(|review| review.lane != "security" && review.lane != "performance")
    );
    // A retry asks for the missing lanes and starts only those; the cut lane gains no second round.
    let (retried, error) = w
        .lead(
            "crew_review",
            json!({ "task": subject, "lanes": ["security", "performance"] }),
        )
        .await;
    assert!(!error, "{retried}");
    for lane in retried["lanes"].as_array().unwrap() {
        let task = lane["task"].as_str().unwrap();
        w.until_state(task, "passed").await;
    }
    let reviews = w.core.store.crew_reviews_of(&subject).unwrap();
    assert_eq!(
        reviews.iter().filter(|r| r.lane == "correctness").count(),
        1
    );
    assert_eq!(reviews.iter().filter(|r| r.lane == "security").count(), 1);
    assert_eq!(
        reviews.iter().filter(|r| r.lane == "performance").count(),
        1
    );
}

/// Issue #628, the blocker: the loop is paced by the deadline of the MCP timer itself. Run inside the real
/// `within`, with a reserve that leaves no room after the first lane, the call answers with the cut error
/// and the timer never fires, so the generic "did not finish in time" is not what comes back.
///
/// The budget is the test build's own clock: the first lane's start is real git and real routing, and a loaded
/// machine can take longer than a production-sized timer (tdd 3.7 reproduced this flake as the timer's generic
/// "did not finish in time" reply). The reserve stays larger than the budget, so the boundary pinned is the same
/// one: no room for a second lane, so the call cuts itself before the timer can fire.
#[tokio::test]
async fn a_review_call_paced_by_the_mcp_timer_names_its_lanes() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        _ => reviewed(vec![]),
    }))
    .await;
    let subject = w.built_work().await;
    let binding = lead_binding(&w);
    let args = json!({ "task": subject, "lanes": ["correctness", "security", "performance"] });
    let budget = std::time::Duration::from_secs(10 * 60);
    let reply = crate::tools::mcp::within_deadline(budget, async {
        let deadline = crate::tools::mcp::call_deadline().expect("a budgeted call has a deadline");
        assert!(deadline <= std::time::Instant::now() + budget);
        let reserve = budget + std::time::Duration::from_secs(5);
        match w
            .core
            .crew_lanes_tool(&binding, &args, deadline, reserve)
            .await
        {
            Ok(value) => Ok(value),
            Err(error) => Ok(json!({ "isError": true, "text": error.message })),
        }
    })
    .await
    .unwrap();
    let text = reply["text"].as_str().unwrap_or_default();
    assert!(
        text.starts_with("lanes: the tool budget ran out. Started: correctness ("),
        "{reply}"
    );
    assert!(
        text.contains("Not reached: security, performance."),
        "{reply}"
    );
    assert!(!text.contains("did not finish in time"));
    let mut reviews = w.core.store.crew_reviews_of(&subject).unwrap();
    assert_eq!(reviews.len(), 1, "only the first lane started");
    let started = reviews.pop().expect("the lane the cut call started");
    assert_eq!(
        started.lane, "correctness",
        "the call starts its first lane"
    );
    w.until_state(&started.task_id, "passed").await;
}

pub(super) fn commit_rules(w: &super::harness::World, text: &str) {
    std::fs::create_dir_all(w.repo.join(".orbyn")).unwrap();
    std::fs::write(w.repo.join(".orbyn/crew.json"), text).unwrap();
    commit_all(&w.repo, "rules");
}

/// The rule file the two `repo_rules_add_lanes` branches share: one lane a change under `src/` asks for, one only a
/// change under `docs/` does (so it must not run), and one every change asks for.
fn rule_lanes() -> String {
    json!({ "required": ["checks"], "lanes": [
        { "lane": "security", "when": { "paths": ["src/**"] }, "locked": true },
        { "lane": "docs", "when": { "paths": ["docs/**"] } },
        { "lane": "licence" },
    ] })
    .to_string()
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
    commit_rules(&w, &rule_lanes());
    let subject = w.built_work().await;
    // The call is paced by the real 45-second MCP budget: four lane starts, each a few git calls, can run past it on a
    // loaded machine, and the answer is then the cut error instead of the lanes (#1159). This test is about the rules,
    // so it sets the clock out of reach; the cut branch is `a_cut_rule_lane_call_names_the_lanes_it_did_not_reach`.
    let answer = w
        .review_at(
            json!({ "task": subject, "lanes": ["correctness"] }),
            std::time::Instant::now() + std::time::Duration::from_secs(3600),
            std::time::Duration::ZERO,
        )
        .await
        .unwrap_or_else(|error| panic!("the review call was cut or refused: {error}"));
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
    // The route reports why: the merge-owed code lane and the locked security lane stay lead, the others default to the worker tier.
    // (The fake engines have one model, which the harness assigns the lead tier, so the reason is what tells the lanes apart.)
    let why: Vec<(&str, bool)> = answer["lanes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| (l["lane"].as_str().unwrap(), stays_lead(l)))
        .collect();
    assert_eq!(
        why,
        [
            ("correctness", true),
            ("security", true),
            ("licence", false)
        ]
    );
}

/// The other branch of the same call: when the clock runs out, the answer names the lanes the repo's rules added and did
/// not reach, so a lead knows to ask again. The deadline is already reached, so this is the state a loaded machine hit in
/// #1159, asserted on purpose instead of waited for.
#[tokio::test]
async fn a_cut_rule_lane_call_names_the_lanes_it_did_not_reach() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        _ => reviewed(vec![]),
    }))
    .await;
    commit_rules(&w, &rule_lanes());
    let subject = w.built_work().await;
    let cut = w
        .review_at(
            json!({ "task": subject, "lanes": ["correctness"] }),
            std::time::Instant::now(),
            std::time::Duration::ZERO,
        )
        .await
        .unwrap_err();
    assert!(
        cut.message
            .starts_with("lanes: the tool budget ran out. Started: correctness ("),
        "{cut}"
    );
    assert!(
        cut.message.contains("Not reached: security, licence."),
        "{cut}"
    );
}

fn stays_lead(lane: &serde_json::Value) -> bool {
    lane["route"]["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["code"] == "review_lane_tier")
        .is_some_and(|r| {
            r["text"]
                .as_str()
                .unwrap()
                .contains("stays on the lead tier")
        })
}

/// A lane a rule locks keeps the lead tier even when its name is an ordinary one.
#[tokio::test]
async fn a_locked_rule_lane_keeps_the_lead_tier() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        _ => reviewed(vec![]),
    }))
    .await;
    commit_rules(
        &w,
        &json!({ "lanes": [{ "lane": "performance", "locked": true }] }).to_string(),
    );
    let subject = w.built_work().await;
    let (answer, error) = w
        .lead(
            "crew_review",
            json!({ "task": subject, "lanes": ["performance", "docs"] }),
        )
        .await;
    assert!(!error, "{answer}");
    let why: Vec<(&str, bool)> = answer["lanes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| (l["lane"].as_str().unwrap(), stays_lead(l)))
        .collect();
    assert_eq!(why, [("performance", true), ("docs", false)]);
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
    let lane = one_lane(w.review(&subject, &["correctness"]).await);
    w.until_state(&lane, "passed").await;
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
    let gate = w
        .core
        .store
        .crew_gates(&subject)
        .unwrap()
        .into_iter()
        .rev()
        .find(|g| g.stage == GateStage::FindingCheck)
        .unwrap();
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
    let review = w
        .core
        .store
        .crew_reviews_of(&subject)
        .unwrap()
        .into_iter()
        .find(|review| review.role == "review" && review.lane == "correctness")
        .expect("the fixture asks for the correctness lane");
    let first = w
        .core
        .store
        .crew_route_decisions(&review.task_id)
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
    let gate = w
        .core
        .store
        .crew_gates(&subject)
        .unwrap()
        .into_iter()
        .rev()
        .find(|g| g.stage == GateStage::FindingCheck)
        .unwrap();
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
    let lane = one_lane(w.review(&subject, &["correctness"]).await);
    let thread = w.worker(&lane);
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

/// Not knowing never lowers a review: no base commit, a rules file that cannot be read, or no merge entry keep the lead tier.
#[tokio::test]
async fn lane_tier_checks_fail_closed() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        _ => reviewed(vec![]),
    }))
    .await;
    commit_rules(&w, r#"{"lanes":[{"lane":"licence","locked":true}]}"#);
    let subject = w.built_work().await;
    let base = w
        .core
        .store
        .crew_run(&subject)
        .unwrap()
        .unwrap()
        .base_commit;
    let token = |t: &str| orbyn_contracts::CrewToken::new(t).unwrap();
    async fn locked_at(root: &std::path::Path, base: Option<&str>, lane: &str) -> bool {
        let lane = orbyn_contracts::CrewToken::new(lane).unwrap();
        crate::crew::review::lane_locked(root, base, &lane).await
    }
    let repo = w.repo.clone();
    macro_rules! locked {
        ($base:expr, $lane:expr) => {
            locked_at(&repo, $base, $lane)
        };
    }
    assert!(locked!(base.as_deref(), "licence").await);
    assert!(!locked!(base.as_deref(), "docs").await);
    assert!(locked!(None, "docs").await, "no base commit: locked");
    assert!(
        locked!(Some("0123456789012345678901234567890123456789"), "docs").await,
        "rules unreadable at an unknown commit: locked"
    );
    // A rules file that does not parse is unreadable too.
    commit_rules(&w, "not json");
    let head = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&w.repo)
        .output()
        .unwrap();
    let head = String::from_utf8(head.stdout).unwrap();
    assert!(
        locked!(Some(head.trim()), "docs").await,
        "invalid rules: locked"
    );
    // No merge entry, so a merge may be owed the code lane: it keeps the lead tier; another lane does not.
    let (root, base) = (w.repo.clone(), base.as_deref());
    assert!(
        w.core
            .lane_keeps_lead(&root, base, Some(&subject), &token("correctness"))
            .await
    );
    assert!(
        w.core
            .lane_keeps_lead(&root, base, None, &token("correctness"))
            .await
    );
    assert!(
        !w.core
            .lane_keeps_lead(&root, base, Some(&subject), &token("docs"))
            .await
    );
    assert!(
        w.core
            .lane_keeps_lead(&root, base, Some(&subject), &token("licence"))
            .await
    );
}
