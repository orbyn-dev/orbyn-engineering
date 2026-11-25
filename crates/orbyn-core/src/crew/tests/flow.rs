//! The Flow reads (Crew Preview) on fake engines: the cells of a task's line follow the task through its review, a
//! reviewer's run is a lane and not a row, the reads change nothing, the flag off hides them, and a task that is not
//! final never shows check text.

use orbyn_contracts::{
    CrewObject, FlowCellState as State, FlowStage as Stage, FlowTaskDetail, FlowTaskList,
    FlowTaskRow, object_id,
};
use serde_json::{Value, json};

use super::harness::{KEY_MARKER, Turn, brief, key, world};
use super::review_support::{arc, built, crew, finding, reviewed};
use crate::crew::Page;
use crate::store::FindingRepro;

/// A second finished task in the same crew. A task's id is the hash of its brief, which carries the second it was written
/// in, so two identical briefs started together are one task; this one has an outcome of its own.
pub(super) async fn built_another(w: &super::harness::World, outcome: &str, key: Value) -> String {
    let (answer, error) = w
        .start(
            brief(
                json!({ "outcome": outcome, "engine": { "id": "alpha", "assigned_by": "person" } }),
            ),
            key,
        )
        .await;
    assert!(!error, "the second task did not start");
    let id = answer["task"].as_str().unwrap().to_owned();
    w.until_state(&id, "passed").await;
    id
}

pub(super) fn page() -> Page {
    Page::new(None, None).unwrap()
}

/// The list's row for `task`, found by id: the read owns the row order, so a test does not.
fn task_row<'a>(list: &'a FlowTaskList, task: &str) -> &'a FlowTaskRow {
    list.tasks
        .iter()
        .find(|t| t.id == task)
        .expect("the task's row in its crew's list")
}

fn state(list: &FlowTaskList, task: &str, stage: Stage) -> State {
    task_row(list, task)
        .cells
        .iter()
        .find(|c| c.stage == stage)
        .unwrap()
        .state
}

/// Every row of every crew table, by table, read from the store's own connection.
pub(super) fn crew_rows(w: &super::harness::World) -> Vec<(String, i64)> {
    let conn = w.core.store.reader();
    let names: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE 'crew\\_%' ESCAPE '\\' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    names
        .into_iter()
        .map(|name| {
            let count = conn
                .query_row(&format!("SELECT COUNT(*) FROM {name}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            (name, count)
        })
        .collect()
}

/// Every Flow read for a task and its crew, as one text, so a leak anywhere shows.
fn everything(w: &super::harness::World, crew_id: &str, task: &str) -> String {
    let list = w
        .core
        .crew_flow_tasks(crew_id, None, None, None, page())
        .unwrap();
    let detail = w.core.crew_flow_task(task).unwrap();
    let crews = w.core.crew_flow_crews(page()).unwrap();
    // Each lane on its own, named as the zoom shows it.
    let lanes: Vec<String> = detail
        .lanes
        .iter()
        .map(|lane| {
            serde_json::to_string(&w.core.crew_flow_lane(task, &lane.lane).unwrap()).unwrap()
        })
        .collect();
    format!(
        "{} {} {} {}",
        serde_json::to_string(&list).unwrap(),
        serde_json::to_string(&detail).unwrap(),
        serde_json::to_string(&crews).unwrap(),
        lanes.join(" ")
    )
}

#[tokio::test]
async fn the_cells_follow_a_task_through_its_review_and_the_reads_change_nothing() {
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
    let crew_id = w.task(&subject).crew_id;

    // Built and checked, nothing reviewed yet: the line stops at Checks.
    let before = crew_rows(&w);
    let list = w
        .core
        .crew_flow_tasks(&crew_id, None, None, None, page())
        .unwrap();
    assert_eq!(list.tasks.len(), 1);
    let row = task_row(&list, &subject);
    assert_eq!(row.cells.len(), 10);
    assert_eq!(state(&list, &subject, Stage::Build), State::Done);
    let checks = row.cells.iter().find(|c| c.stage == Stage::Checks).unwrap();
    assert_eq!(checks.state, State::Done);
    assert!(checks.passed.unwrap() >= 1 && checks.passed <= checks.of);
    assert_eq!(state(&list, &subject, Stage::Code), State::Pending);
    assert_eq!(state(&list, &subject, Stage::Merge), State::Pending);
    assert_eq!(list.counts.passed, 1);

    // Two lanes run: the reviewers' tasks are lanes of this task, not rows of the list.
    let lanes = w.review(&subject, &["correctness", "security"]).await;
    for lane in &lanes {
        w.until_state(lane, "passed").await;
    }
    let list = w
        .core
        .crew_flow_tasks(&crew_id, None, None, None, page())
        .unwrap();
    assert_eq!(list.tasks.len(), 1, "a review run is not a work task");
    assert_eq!(list.total, 1);
    assert_eq!(state(&list, &subject, Stage::Code), State::Done);
    assert_eq!(state(&list, &subject, Stage::Security), State::Done);
    assert_eq!(task_row(&list, &subject).round, 1);

    let detail = w.core.crew_flow_task(&subject).unwrap();
    assert_eq!(detail.lanes.len(), 2);
    let lane = detail
        .lanes
        .iter()
        .find(|lane| lane.group == "code")
        .expect("the code lane");
    assert_eq!(
        serde_json::to_value(lane)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>(),
        [
            "commit",
            "confirmed",
            "engine",
            "findings",
            "group",
            "lane",
            "model",
            "round",
            "same_engine",
            "same_engine_allowed_by",
            "state",
            "task",
            "tier",
            "unconfirmed"
        ]
    );
    assert_eq!(lane.group, "code");
    assert_eq!(
        lane.tier.as_deref(),
        Some("lead"),
        "the code lane is owed to a merge"
    );
    let security = detail.lanes.iter().find(|l| l.group == "security").unwrap();
    assert_eq!(security.tier.as_deref(), Some("lead"));
    assert!(
        lane.same_engine_allowed_by.is_none(),
        "a cross-vendor review has no same-engine consent authority"
    );
    assert!(lane.commit.as_ref().is_some_and(|c| !c.is_empty()));
    assert_eq!(
        lane.same_engine,
        Some(false),
        "alpha built and beta reviewed"
    );
    assert!(detail.lanes.iter().any(|l| l.group == "security"));
    assert!(
        detail
            .lanes
            .iter()
            .all(|l| l.engine.as_deref() == Some("beta"))
    );
    assert_eq!(detail.findings.len(), 2, "each lane filed the finding");
    assert!(
        detail
            .findings
            .iter()
            .all(|f| !f.blocks && f.check == "unchecked")
    );
    assert_eq!(detail.rounds.len(), 1);
    assert_eq!(detail.brief.purpose, "Add a parse_duration function.");
    // A reviewer's task is not a task to zoom into.
    for lane in &lanes {
        assert!(
            w.core.crew_flow_task(lane).is_err(),
            "a reviewer's task zoomed"
        );
    }

    // A lane is read on its own: the row the zoom shows, what it was given, and the finding it filed with the
    // reviewer's own evidence.
    let one = w.core.crew_flow_lane(&subject, "correctness").unwrap();
    assert_eq!(one.task, subject);
    assert_eq!(one.crew, crew_id);
    assert_eq!(
        (one.lane.lane.as_str(), one.lane.group.as_str()),
        ("correctness", "code")
    );
    assert_eq!(one.lane.findings, 1);
    assert_eq!(one.lane.confirmed, 0, "the finding is unchecked");
    assert!(one.lane.commit.is_some(), "the commit the lane read");
    assert!(one.lane.task != subject, "the lane's own task is named");
    assert_eq!(one.given.purpose, "Add a parse_duration function.");
    assert!(one.given.checks_of.is_some_and(|of| of >= 1));
    assert!(one.given.checks_passed.is_some());
    assert_eq!(one.given.commit, one.lane.commit);
    assert_eq!(one.findings.len(), 1);
    let filed = one
        .findings
        .iter()
        .find(|finding| finding.path == "src/lib.txt")
        .expect("the correctness lane's finding");
    assert_eq!((filed.lane.as_str(), filed.line), ("correctness", 1));
    assert_eq!(filed.claim, "It wraps on a large value.");
    assert_eq!(filed.evidence, "Reading the code shows it.");
    assert_eq!(filed.severity, "high");
    assert!(!one.truncated);
    // The security lane is read for itself and does not carry the code lane's finding.
    let security = w.core.crew_flow_lane(&subject, "security").unwrap();
    assert_eq!(security.lane.group, "security");
    assert!(security.findings.iter().all(|f| f.lane == "security"));
    // A lane that did not run, a reviewer's own task and a name that is not a lane are refused.
    for lane in &lanes {
        assert!(
            w.core.crew_flow_lane(lane, "correctness").is_err(),
            "a reviewer's own task answered for {lane}"
        );
    }
    for (task, lane) in [
        (&subject, "nope"),
        (&subject, "Correctness"),
        (&subject, "a lane"),
    ] {
        assert!(
            w.core.crew_flow_lane(task, lane).is_err(),
            "{lane} answered for {task}"
        );
    }

    // Filters: nothing needs the person, so that filter is empty and the counts still say all.
    let none = w
        .core
        .crew_flow_tasks(&crew_id, Some("needs_you"), None, None, page())
        .unwrap();
    assert!(none.tasks.is_empty() && none.counts.all == 1);
    let by_engine = w
        .core
        .crew_flow_tasks(&crew_id, None, Some("alpha"), Some("PARSE"), page())
        .unwrap();
    assert_eq!(by_engine.tasks.len(), 1);
    let other = w
        .core
        .crew_flow_tasks(&crew_id, None, Some("beta"), None, page())
        .unwrap();
    assert!(other.tasks.is_empty());
    assert!(
        w.core
            .crew_flow_tasks(&crew_id, Some("nothing"), None, None, page())
            .is_err()
    );
    let crews = w.core.crew_flow_crews(page()).unwrap();
    assert_eq!(crews.crews.len(), 1);
    let listed = crews
        .crews
        .iter()
        .find(|crew| crew.id == crew_id)
        .expect("the crew's row");
    assert_eq!(listed.tasks, 1);

    // Reading changed no row of any crew table, and no task's state or time.
    let moved = w.task(&subject).updated_at;
    let rows = crew_rows(&w);
    for _ in 0..3 {
        everything(&w, &crew_id, &subject);
    }
    assert_eq!(rows, crew_rows(&w));
    assert_eq!(moved, w.task(&subject).updated_at);
    assert!(before.len() >= 20, "every crew table was counted");
}

#[tokio::test]
async fn the_answer_key_and_a_checks_text_are_never_in_a_read() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        _ => reviewed(vec![]),
    }))
    .await;
    let subject = w.built_work().await;
    let crew_id = w.task(&subject).crew_id;
    let lanes = w.review(&subject, &["correctness"]).await;
    for lane in &lanes {
        w.until_state(lane, "passed").await;
    }

    let stored = w.core.store.crew_evidence(&subject).unwrap();
    assert!(!stored.is_empty());
    let key = w.core.store.crew_answer_key(&subject).unwrap().unwrap();
    let mut forbidden: Vec<String> = vec![key.id.as_str().to_owned()];
    for e in &stored {
        forbidden.extend([
            e.command.as_str().to_owned(),
            e.proves.as_str().to_owned(),
            e.does_not_prove.as_str().to_owned(),
            e.check.as_str().to_owned(),
        ]);
        if !e.excerpt.is_empty() {
            forbidden.push(e.excerpt.clone());
        }
    }
    for check in &key.checks {
        forbidden.push(check.id.as_str().to_owned());
    }
    let text = everything(&w, &crew_id, &subject);
    for (n, needle) in forbidden.iter().filter(|n| n.len() > 3).enumerate() {
        assert!(
            !text.contains(needle.as_str()),
            "a read holds stored check text number {n}"
        );
    }
    for field in ["worktree", "thread", "protected", "repro", "answer_key"] {
        assert!(!text.contains(field), "a read names {field}");
    }
    let worktree = w.core.store.crew_run(&subject).unwrap().unwrap().worktree;
    if let Some(path) = worktree {
        assert!(!text.contains(&path), "a read holds a path on this machine");
    }
}

#[tokio::test]
async fn a_findings_evidence_is_withheld_and_cut_and_keeps_a_path() {
    // A key's command is a needle: longer than the eight characters `withhold` matches, it is replaced wherever it is
    // quoted. The absolute path is not: nothing strips one from a text a Flow screen reads, here or in a claim.
    let command = format!("echo {KEY_MARKER}");
    let path = "/home/someone/repo/src/lib.rs";
    let tail = "words ".repeat(200).trim_end().to_owned();
    let written = format!("The command {command} ran on {path}. {tail}");
    let mut draft = finding("src/lib.txt", 1, "It wraps on a large value.");
    draft["evidence"] = json!(written);
    let w = crew(arc(move |n, root| match n {
        0 => built(root),
        _ => reviewed(vec![draft.clone()]),
    }))
    .await;
    let subject = w.built_work().await;
    let crew_id = w.task(&subject).crew_id;
    let lanes = w.review(&subject, &["correctness"]).await;
    for lane in &lanes {
        w.until_state(lane, "passed").await;
    }

    // What the reviewer wrote is stored whole; only a read scrubs it.
    let stored = w.core.store.crew_findings_of(&subject).unwrap();
    assert_eq!(stored.len(), 1);
    let filed = stored
        .iter()
        .find(|finding| finding.finding.claim.as_str() == "It wraps on a large value.")
        .expect("the filed finding");
    assert_eq!(filed.finding.evidence.as_str(), written);
    let key_id = w
        .core
        .store
        .crew_answer_key(&subject)
        .unwrap()
        .unwrap()
        .id
        .as_str()
        .to_owned();

    let detail = w.core.crew_flow_task(&subject).unwrap();
    let filed = detail
        .findings
        .iter()
        .find(|finding| finding.claim == "It wraps on a large value.")
        .expect("the filed finding");
    let evidence = &filed.evidence;
    assert!(
        !evidence.contains(&command),
        "the key's command reached a read: {evidence}"
    );
    assert!(
        evidence.contains("[check]"),
        "it is withheld, not dropped: {evidence}"
    );
    assert!(
        evidence.contains(path),
        "an absolute path is not stripped: {evidence}"
    );
    assert!(
        evidence.chars().count() <= 300,
        "the text is cut at 300: {}",
        evidence.chars().count()
    );
    assert_ne!(
        evidence, "Withheld.",
        "withholding happens before the leak check"
    );
    // The lane read serves the same scrubbed text, and never the key's id.
    let one = w.core.crew_flow_lane(&subject, "correctness").unwrap();
    let filed = one
        .findings
        .iter()
        .find(|finding| finding.claim == "It wraps on a large value.")
        .expect("the lane's finding");
    assert_eq!(&filed.evidence, evidence);
    assert!(!filed.evidence.contains(&key_id));
    // The crew's whole read surface is held to it, the lane read included.
    let everything = everything(&w, &crew_id, &subject);
    assert!(
        !everything.contains(KEY_MARKER),
        "the key's command reached a read"
    );
    assert!(!everything.contains(&key_id), "the key's id reached a read");
}

#[tokio::test]
async fn a_task_that_is_not_final_shows_no_check_text_and_no_gate_reason() {
    // The first task finishes and leaves Evidence and a Gate; the second never ends.
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        _ => Turn::Hang,
    }))
    .await;
    let done = w.built_work().await;
    let (answer, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "the second task did not start");
    let running = answer["task"].as_str().unwrap().to_owned();
    let task = w.task(&running);
    assert_eq!(task.state, "running");

    // Give the running task Evidence and a Gate that carry text a reader must not see, built at run time.
    let nonce = format!(
        "nonce-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    // Template rows: any row of the finished task works, because the graft below replaces every field
    // a read looks at.
    let evidence = w.core.store.crew_evidence(&done).unwrap().remove(0);
    let gate = w.core.store.crew_gates(&done).unwrap().remove(0);
    for (object, fields) in [
        (
            CrewObject::Evidence(evidence),
            vec![
                ("check", "chk-graft".to_owned()),
                ("command", format!("run {nonce}")),
                ("excerpt", format!("out {nonce}")),
                ("proves", format!("proves {nonce}")),
                ("does_not_prove", format!("not {nonce}")),
            ],
        ),
        (
            CrewObject::Gate(gate),
            vec![("reason", format!("reason {nonce}"))],
        ),
    ] {
        let mut value: Value = object.to_value();
        value["run_id"] = json!(task.run_id);
        value["crew_id"] = json!(task.crew_id);
        for (key, text) in fields {
            value[key] = json!(text);
        }
        value["id"] = json!(object_id(&value).unwrap());
        w.core
            .store
            .add_crew_object(&running, &CrewObject::parse(&value).unwrap())
            .unwrap();
    }

    let text = everything(&w, &task.crew_id, &running);
    assert!(
        !text.contains(&nonce),
        "check text reached a read of a task that is running"
    );
    let detail: FlowTaskDetail = w.core.crew_flow_task(&running).unwrap();
    assert!(detail.gates.iter().all(|g| g.reason.is_none()));
    // The finished one may show its gates' reasons.
    let finished = w.core.crew_flow_task(&done).unwrap();
    assert!(finished.gates.iter().all(|g| g.reason.is_some()));
    // Running sorts nowhere special, and the line says so.
    let list = w
        .core
        .crew_flow_tasks(&task.crew_id, None, None, None, page())
        .unwrap();
    assert_eq!(list.tasks.len(), 2);
    let row = list.tasks.iter().find(|t| t.id == running).unwrap();
    assert_eq!(
        row.cells
            .iter()
            .find(|c| c.stage == Stage::Build)
            .unwrap()
            .state,
        State::Running
    );
}

#[tokio::test]
async fn a_merge_waiting_for_the_person_needs_you_and_rises_to_the_top() {
    let w = crew(arc(|_, root| built(root))).await;
    let first = w.built_work().await;
    let crew_id = w.task(&first).crew_id;
    let second = built_another(&w, "Add a second function.", key()).await;
    assert_ne!(first, second);
    // The older task waits for a person at the merge queue.
    let entry = crate::crew::fixtures::merge_entry(&first, "/repo", "waiting");
    w.core.store.add_merge_entry(&entry).unwrap();

    let list = w
        .core
        .crew_flow_tasks(&crew_id, None, None, None, page())
        .unwrap();
    // Order is the read's contract here: the task that needs the person comes first.
    assert_eq!(
        list.tasks[0].id, first,
        "the row that needs the person comes first"
    );
    let merge = list
        .tasks
        .iter()
        .find(|t| t.id == first)
        .expect("the first row")
        .cells
        .iter()
        .find(|c| c.stage == Stage::Merge)
        .unwrap();
    assert_eq!(merge.state, State::NeedsYou);
    assert_eq!(list.counts.needs_you, 1);
    let only = w
        .core
        .crew_flow_tasks(&crew_id, Some("needs_you"), None, None, page())
        .unwrap();
    assert_eq!(only.tasks.len(), 1);
    // A page of one still says where the next starts.
    let paged = w
        .core
        .crew_flow_tasks(
            &crew_id,
            None,
            None,
            None,
            Page::new(Some(1), None).unwrap(),
        )
        .unwrap();
    assert_eq!(
        (paged.tasks.len(), paged.total, paged.next_offset),
        (1, 2, Some(1))
    );
    let detail = w.core.crew_flow_task(&first).unwrap();
    assert_eq!(detail.merge.unwrap().state, "waiting");
}

#[tokio::test]
async fn with_the_flag_off_the_reads_are_not_found() {
    let w = world(false, arc(|_, _| Turn::Silent)).await;
    assert!(matches!(
        w.core.crew_flow_crews(page()),
        Err(crate::error::CoreError::NotFound(_))
    ));
    assert!(matches!(
        w.core.crew_flow_tasks("crew-1", None, None, None, page()),
        Err(crate::error::CoreError::NotFound(_))
    ));
    assert!(matches!(
        w.core.crew_flow_task("sha256:abc"),
        Err(crate::error::CoreError::NotFound(_))
    ));
}

#[tokio::test]
async fn bad_input_is_refused_and_an_empty_crew_is_an_empty_list() {
    let w = crew(arc(|_, root| built(root))).await;
    for crew in ["", "a b", "../x", &"x".repeat(97)] {
        assert!(
            w.core
                .crew_flow_tasks(crew, None, None, None, page())
                .is_err(),
            "{crew}"
        );
    }
    assert!(w.core.crew_flow_task("../etc").is_err());
    assert!(
        w.core
            .crew_flow_tasks("crew-1", None, None, Some(&"q".repeat(81)), page())
            .is_err()
    );
    let empty = w
        .core
        .crew_flow_tasks("nobody", None, None, None, page())
        .unwrap();
    assert!(empty.tasks.is_empty() && empty.counts.all == 0 && !empty.truncated);
    assert!(matches!(
        w.core.crew_flow_task("sha256:ffff"),
        Err(crate::error::CoreError::NotFound(_))
    ));
}

/// A runtime value that is unique to this run and appears in no constant.
pub(super) fn nonce(label: &str) -> String {
    format!(
        "{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

#[tokio::test]
async fn a_gate_reason_that_quotes_a_repro_or_another_tasks_check_is_withheld() {
    let w = crew(arc(|n, root| match n {
        1 => reviewed(vec![finding(
            "src/lib.txt",
            1,
            "It wraps on a large value.",
        )]),
        _ => built(root),
    }))
    .await;
    let subject = w.built_work().await;
    let crew_id = w.task(&subject).crew_id;
    let lanes = w.review(&subject, &["correctness"]).await;
    for lane in &lanes {
        w.until_state(lane, "passed").await;
    }

    // Another task of the crew has a key whose check is a command of its own.
    let other_check = format!("echo {}", nonce("other-check"));
    let other_key = json!({
        "checks": [{ "id": "chk-other", "kind": "command", "params": { "run": other_check },
            "proves": "It runs." }],
        "protected_paths": ["tests/**"]
    });
    let other = built_another(&w, "Add another function.", other_key).await;
    assert_ne!(other, subject);

    // The lead's repro for the finding is stored apart from any key.
    let repro_command = format!("echo {}", nonce("repro"));
    let repro = json!({ "id": "repro-x", "kind": "command", "params": { "run": repro_command },
        "proves": "It fails." });
    w.core
        .store
        .add_finding_repro(&FindingRepro {
            finding_id: w.first_finding(&subject),
            subject_id: subject.clone(),
            repro: repro.to_string(),
        })
        .unwrap();

    // The lead's free-text reason on a Gate is checked for secrets when written, not for key text.
    // Any gate of the task is a template: its reason is replaced below and its id recomputed.
    let mut gate =
        CrewObject::Gate(w.core.store.crew_gates(&subject).unwrap().remove(0)).to_value();
    gate["reason"] = json!(format!("ran {repro_command} and {other_check}"));
    gate["id"] = json!(object_id(&gate).unwrap());
    w.core
        .store
        .add_crew_object(&subject, &CrewObject::parse(&gate).unwrap())
        .unwrap();
    assert_eq!(
        w.task(&subject).state,
        "passed",
        "the task is final, so reasons are read"
    );

    let text = everything(&w, &crew_id, &subject);
    assert!(
        text.contains("ran "),
        "the reason is read once the task is final"
    );
    for (label, secret) in [
        ("the repro command", &repro_command),
        ("the other task's check", &other_check),
    ] {
        assert!(!text.contains(secret.as_str()), "a read holds {label}");
    }
}

#[tokio::test]
async fn only_the_security_lane_is_the_security_review() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        _ => reviewed(vec![]),
    }))
    .await;
    let subject = w.built_work().await;
    let crew_id = w.task(&subject).crew_id;
    let lanes = w.review(&subject, &["secrets", "keys"]).await;
    for lane in &lanes {
        w.until_state(lane, "passed").await;
    }
    let list = w
        .core
        .crew_flow_tasks(&crew_id, None, None, None, page())
        .unwrap();
    assert_eq!(
        state(&list, &subject, Stage::Security),
        State::Pending,
        "a lane named secrets is not the Security review"
    );
    assert_eq!(state(&list, &subject, Stage::Code), State::Done);
    let detail = w.core.crew_flow_task(&subject).unwrap();
    assert!(detail.lanes.iter().all(|l| l.group == "code"));
}
