//! G1, "steer, never queue": a lead's follow-up to a worker goes into the running turn, never
//! behind it in the thread's person queue. The one path that queues toward a worker is the
//! core's retry brief after a failed verification, and it is queued only once the worker's turn
//! has ended.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};

use crate::agents::driver::SteerMessage;

use super::bus_support::{Duo, duo};
use super::harness::{
    Turn, World, brief, key, report, world_refusing_steers, world_refusing_steers_after, world_with,
};

/// Every kind a lead can send a worker, in the same order as `MessageKind`.
const KINDS: [&str; 5] = ["fact", "question", "handoff", "blocker", "answer"];

fn body(state: &str, kind: &str) -> String {
    format!("{state} {kind} from the lead")
}

fn queue(w: &World, thread: &str) -> Vec<String> {
    w.core
        .store
        .list_queue(thread)
        .unwrap()
        .into_iter()
        .map(|message| message.prompt)
        .collect()
}

/// What a running turn's input was handed for this thread, as the fake engine received it.
fn steered_to(w: &World, thread: &str) -> Vec<String> {
    w.steered
        .lock()
        .unwrap()
        .iter()
        .filter(|(to, _)| to == thread)
        .map(|(_, text)| text.clone())
        .collect()
}

fn running(w: &World, thread: &str) -> bool {
    w.core
        .store
        .thread(thread)
        .unwrap()
        .active_run
        .is_some_and(|run| run.state == orbyn_contracts::RunState::Running)
}

fn message(to: &str, kind: &str, body: &str, reply: Option<&str>) -> Value {
    let mut args = json!({ "to": to, "kind": kind, "body": body });
    if let Some(id) = reply {
        args["in_reply_to"] = json!(id);
    }
    args
}

async fn post_from_lead(d: &Duo, args: Value) -> Value {
    let (sent, error) = d.call(&d.w.lead_token, "crew_post", args).await;
    assert!(!error, "{sent}");
    sent
}

/// The one row a read returned, unpacked by pattern so a test never reads a place in the list: a
/// read that returns none or several fails here with the rows attached.
fn one<'a, T: std::fmt::Debug>(rows: &'a [T], what: &str) -> &'a T {
    match rows {
        [row] => row,
        [] => panic!("no {what}: {rows:?}"),
        _ => panic!("{} {what}s where one is expected: {rows:?}", rows.len()),
    }
}

/// The delivery row a post to one member answers with, found by the recipient it names: one post
/// answers one row per target, so a second row or a row for another member is a bug, not a place
/// a test may read.
fn delivered_to<'a>(sent: &'a Value, to: &str) -> &'a Value {
    let delivered = sent["delivered"]
        .as_array()
        .expect("a post answers with its delivery rows");
    assert_eq!(
        delivered.len(),
        1,
        "one post to one member answers one delivery row: {sent}"
    );
    delivered
        .iter()
        .find(|row| row["to"].as_str() == Some(to))
        .unwrap_or_else(|| panic!("no delivery to {to}: {sent}"))
}

/// A question the worker posts with `crew_post`, not `crew_ask`: an ask that is waiting takes
/// the answer back inside its own tool call instead of handing it to the running turn.
async fn question_to_lead(d: &Duo) -> String {
    let (asked, error) = d
        .call(
            &d.ta,
            "crew_post",
            json!({ "to": "lead", "kind": "question", "body": "Which route, lead?" }),
        )
        .await;
    assert!(!error, "{asked}");
    asked["id"].as_str().unwrap().to_owned()
}

async fn sent_from_lead(
    d: &Duo,
    state: &str,
    reply: &str,
    expected: &str,
) -> Vec<(String, String)> {
    let mut modes = Vec::new();
    for kind in KINDS {
        let text = body(state, kind);
        let in_reply_to = (kind == "answer").then_some(reply);
        let sent = post_from_lead(d, message(&d.ma, kind, &text, in_reply_to)).await;
        let delivered = delivered_to(&sent, &d.ma);
        assert_eq!(delivered["mode"], expected, "{state} {kind}: {sent}");
        modes.push((kind.to_owned(), delivered["mode"].to_string()));
    }
    modes
}

/// The worker's turn ends and its thread is idle, while the task stays live.
async fn pause(d: &Duo) -> String {
    let worker = d.w.worker(&d.a);
    d.w.core
        .store
        .move_crew_task(&d.a, "needs_person", None)
        .unwrap();
    let active = d.w.core.store.thread(&worker).unwrap().active_run.unwrap();
    d.w.core.runs.cancel(&active.id).unwrap();
    d.w.until("the paused worker's turn ends", |w| {
        w.core.store.thread(&worker).unwrap().active_run.is_none()
    })
    .await;
    worker
}

/// G1: for every kind a lead can send a busy worker, the running turn's input takes the text
/// and the worker's person queue stays empty. The lead's first brief is still a plain prompt.
#[tokio::test]
async fn every_kind_a_lead_sends_a_busy_worker_steers_the_turn_and_queues_nothing() {
    let d = duo().await;
    let worker = d.w.worker(&d.a);
    assert_eq!(
        d.w.sent_to(&worker).len(),
        1,
        "the first brief is the only prompt"
    );
    let reply = question_to_lead(&d).await;
    let modes = sent_from_lead(&d, "live", &reply, "live").await;
    assert_eq!(modes.len(), KINDS.len());
    assert!(queue(&d.w, &worker).is_empty(), "the bus wrote the queue");

    // Every kind was handed to the turn, under the crew's wrapper and role. The fake's receive
    // task records each steer after the channel accepts it, so wait for all of them first.
    d.w.until("every kind is in the turn's input", |_| {
        d.steered_to(&worker).len() >= KINDS.len()
    })
    .await;
    let handed = d.steered_to(&worker);
    for kind in KINDS {
        let text = body("live", kind);
        assert!(
            handed.iter().any(|steered| steered.contains(&text)),
            "{kind} never reached the input: {handed:?}"
        );
    }
    assert_eq!(
        handed.len(),
        KINDS.len(),
        "the bus handed the turn more than the lead's kinds"
    );
    assert_eq!(
        d.w.sent_to(&worker).len(),
        1,
        "the bus started a turn for a worker"
    );
    let rows = d.w.core.store.crew_deliveries(&d.a).unwrap();
    let to_worker: Vec<&str> = rows
        .iter()
        .filter(|row| row.recipient == d.ma)
        .map(|row| row.mode.as_str())
        .collect();
    assert_eq!(to_worker, ["live"; KINDS.len()], "{rows:?}");
}

/// G1: a message to a worker whose task is not running is held, never queued, and reaches the
/// next prompt once. The states below are the ones a worker waits in: being verified, waiting
/// for a person, and between two turns.
#[tokio::test]
async fn every_kind_to_a_worker_that_is_not_running_is_held_and_never_queued() {
    for state in ["verifying", "needs_person", "between_turns"] {
        let d = duo().await;
        let worker = d.w.worker(&d.a);
        match state {
            "verifying" => {
                d.w.core
                    .store
                    .move_crew_task(&d.a, "verifying", None)
                    .unwrap();
            }
            "needs_person" => {
                pause(&d).await;
            }
            _ => {
                pause(&d).await;
                // Let the cancelled run's end handler settle before the task goes live again,
                // so it cannot mistake the moved task for one still waiting on that run.
                tokio::time::sleep(Duration::from_millis(100)).await;
                d.w.core
                    .store
                    .move_crew_task(&d.a, "running", None)
                    .unwrap();
            }
        }
        let reply = question_to_lead(&d).await;
        let steered = d.steered_to(&worker).len();
        sent_from_lead(&d, state, &reply, "held").await;
        assert!(queue(&d.w, &worker).is_empty(), "{state}: wrote the queue");
        assert_eq!(
            d.steered_to(&worker).len(),
            steered,
            "{state}: a held kind reached the input"
        );

        let crew = d.w.task(&d.a).crew_id;
        if state == "between_turns" {
            // The thread is idle and the task live: the next prompt carries each kind once.
            d.w.core
                .runs
                .enqueue(&worker, "Continue after the person's answer.", &[])
                .unwrap();
            d.w.until("the next prompt starts", |w| w.sent_to(&worker).len() == 2)
                .await;
            let prompt = d.w.sent_to(&worker).last().unwrap().clone();
            for kind in KINDS {
                let text = body(state, kind);
                assert_eq!(prompt.matches(&text).count(), 1, "{state} {kind}: {prompt}");
                assert!(
                    prompt.contains("trust=\"data\""),
                    "{state} {kind}: {prompt}"
                );
            }
            assert!(
                queue(&d.w, &worker).is_empty(),
                "{state}: the prompt queued"
            );
        } else {
            let held = d.held_text(&d.a);
            for kind in KINDS {
                let text = body(state, kind);
                assert_eq!(held.matches(&text).count(), 1, "{state} {kind}: {held}");
            }
        }
        assert!(
            d.w.core
                .store
                .held_crew_messages(&crew, &d.ma)
                .unwrap()
                .is_empty(),
            "{state}: a held kind was sent more than once"
        );
    }
}

fn outside_the_lease(root: &std::path::Path) {
    std::fs::create_dir_all(root.join("docs")).unwrap();
    std::fs::write(root.join("docs/steer.txt"), "outside\n").unwrap();
}

/// G1, the structural guard as behaviour: the retry brief after a failed verification is the
/// one thing that queues toward a worker, on its now idle thread, exactly once. A lead message
/// posted during the running turn only steers it: it writes no queue row and is no prompt.
#[tokio::test]
async fn only_the_retry_brief_is_queued_toward_a_worker() {
    let w = world_with(
        true,
        Arc::new(|turn, root| {
            if turn == 0 {
                outside_the_lease(root);
                // Hold the turn open on a question, so the lead's message below is sent while
                // the worker is still working and before it reports.
                Turn::Steps(vec![
                    (
                        "crew_ask",
                        json!({ "to": "lead", "body": "Which route, lead?", "wait_seconds": 40 }),
                    ),
                    ("crew_report", report("done")),
                ])
            } else {
                Turn::Hang
            }
        }),
        &[("alpha", true)],
    )
    .await;
    let (started, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{started}");
    let task = started["task"].as_str().unwrap().to_owned();
    let worker = w.worker(&task);
    let member = w.core.store.crew_run(&task).unwrap().unwrap().member;
    let crew = w.task(&task).crew_id;
    w.until("the worker's first turn", |w| w.sent_to(&worker).len() == 1)
        .await;

    // The question is waiting on the lead, so the turn is still running and takes input.
    w.until("the worker asks the lead", |w| {
        w.core
            .store
            .crew_messages_in_crew(&crew)
            .unwrap()
            .iter()
            .any(|(_, message)| message.body.as_str() == "Which route, lead?")
    })
    .await;
    let question = w
        .core
        .store
        .crew_messages_in_crew(&crew)
        .unwrap()
        .iter()
        .find(|(_, message)| message.body.as_str() == "Which route, lead?")
        .map(|(_, asked)| asked.id.as_str().to_owned())
        .unwrap();
    let text = "Use the /parse route, not /parse/duration.";
    let (sent, error) = w
        .call(
            &w.lead_token,
            "crew_post",
            message(&member, "fact", text, None),
        )
        .await;
    assert!(!error, "{sent}");
    assert_eq!(delivered_to(&sent, &member)["mode"], "live", "{sent}");
    w.until("the lead's message reached the turn", |w| {
        w.steered
            .lock()
            .unwrap()
            .iter()
            .any(|(to, steered)| to == &worker && steered.contains(text))
    })
    .await;
    assert!(
        queue(&w, &worker).is_empty(),
        "a live lead message wrote the queue"
    );

    // Answer the waiting ask so the turn goes on to report, then fail its check.
    let (answered, error) = w
        .call(
            &w.lead_token,
            "crew_post",
            json!({ "to": member, "kind": "answer", "body": "The /parse route.", "in_reply_to": question }),
        )
        .await;
    assert!(!error, "{answered}");

    // The one thing queued toward the worker is its retry brief, and it is the next prompt.
    w.until("the retry brief starts", |w| w.sent_to(&worker).len() == 2)
        .await;
    // Order is the contract here: the first prompt is the first brief, the second its retry brief.
    let prompts = w.sent_to(&worker);
    assert_eq!(prompts.len(), 2, "{prompts:?}");
    assert!(
        prompts[0].contains("Add a parse_duration function."),
        "the first brief is a plain prompt: {}",
        prompts[0]
    );
    assert_eq!(
        prompts
            .iter()
            .filter(|prompt| prompt.contains("Verification failed."))
            .count(),
        1,
        "the retry brief was queued more than once: {prompts:?}"
    );
    let retry = &prompts[1];
    assert!(retry.contains("Verification failed."), "{retry}");
    assert!(retry.contains("docs/steer.txt"), "{retry}");
    assert!(
        !retry.contains(text),
        "the live lead message was repeated in a prompt: {retry}"
    );
    assert!(
        !prompts[0].contains(text),
        "the live lead message was repeated in a prompt: {}",
        prompts[0]
    );
    assert!(
        queue(&w, &worker).is_empty(),
        "the retry brief did not start as exactly one prompt"
    );
    assert_eq!(
        w.task(&task).state,
        "running",
        "the worker is between turns after the retry brief"
    );
}

/// #444: when the engine refuses a live steer, the message gets one more delivery row saying
/// next_turn for the same message and recipient, and the text is still requeued as before.
#[tokio::test]
async fn a_refused_live_steer_records_next_turn_and_still_requeues() {
    let w = world_refusing_steers(true, Arc::new(|_, _| Turn::Hang), &[("alpha", true)]).await;
    let (started, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{started}");
    let task = started["task"].as_str().unwrap().to_owned();
    let worker = w.worker(&task);
    let member = w.core.store.crew_run(&task).unwrap().unwrap().member;
    w.until("the worker's turn runs", |w| running(w, &worker))
        .await;

    // The lead's message reaches the running turn, so its delivery row says live.
    let text = "Use the /parse route, not /parse/duration.";
    let (sent, error) = w
        .call(
            &w.lead_token,
            "crew_post",
            message(&member, "fact", text, None),
        )
        .await;
    assert!(!error, "{sent}");
    assert_eq!(delivered_to(&sent, &member)["mode"], "live", "{sent}");
    let message_id = sent["id"].as_str().unwrap().to_owned();

    // The engine refused the steer: one more row for the same message and recipient says the text
    // goes with the next turn.
    w.until("the refusal is recorded against the message", |w| {
        w.core
            .store
            .crew_deliveries(&task)
            .unwrap()
            .iter()
            .any(|row| row.message_id == message_id && row.mode == "next_turn")
    })
    .await;
    let rows: Vec<_> = w
        .core
        .store
        .crew_deliveries(&task)
        .unwrap()
        .into_iter()
        .filter(|row| row.message_id == message_id)
        .collect();
    let modes: Vec<&str> = rows.iter().map(|row| row.mode.as_str()).collect();
    assert_eq!(modes, ["live", "next_turn"], "{rows:?}");
    let refused = rows
        .iter()
        .find(|row| row.mode == "next_turn")
        .expect("the refused steer's next_turn row");
    assert_eq!(refused.recipient, member, "{rows:?}");
    assert_eq!(
        refused.note.as_deref(),
        Some("the engine refused the live steer; the text comes with the next turn"),
        "{rows:?}"
    );

    // The text is still requeued as today, wrapper and all, for the next turn.
    let queued = queue(&w, &worker);
    assert!(
        queued.iter().any(|prompt| prompt.contains(&message_id)),
        "{queued:?}"
    );
}

/// #444: a refusal is matched by the engine's steer id, never the text. A person's steer with
/// exactly the same text as a live crew message must not consume the crew message's hand-over.
#[tokio::test]
async fn a_person_steer_with_the_same_text_never_consumes_a_crew_hand_over() {
    // Each fake engine takes its first steer and refuses the next: the crew's reaches the running
    // turn and is accepted, while the person's is refused.
    let w =
        world_refusing_steers_after(true, Arc::new(|_, _| Turn::Hang), &[("alpha", true)], 1).await;
    let (started, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{started}");
    let task = started["task"].as_str().unwrap().to_owned();
    let worker = w.worker(&task);
    let member = w.core.store.crew_run(&task).unwrap().unwrap().member;
    w.until("the worker's turn runs", |w| running(w, &worker))
        .await;

    let (sent, error) = w
        .call(
            &w.lead_token,
            "crew_post",
            message(&member, "fact", "Same words, two senders.", None),
        )
        .await;
    assert!(!error, "{sent}");
    assert_eq!(delivered_to(&sent, &member)["mode"], "live", "{sent}");
    let message_id = sent["id"].as_str().unwrap().to_owned();
    w.until("the crew steer is handed over", |w| {
        steered_to(w, &worker).len() == 1
    })
    .await;
    // The exact text the engine was handed for the crew message, now sent by the person.
    let text = one(&steered_to(&w, &worker), "crew steer handed to the turn").clone();
    assert_eq!(
        w.core.crew_bus.live_steers.waiting().len(),
        1,
        "the crew hand-over is not waiting"
    );

    w.core.runs.steer(&worker, None, &text).unwrap();
    w.until("the person's refused steer is requeued", |w| {
        queue(w, &worker).iter().any(|prompt| prompt == &text)
    })
    .await;

    // The refusal belongs to the person's steer: it writes no row for the crew message and leaves the
    // crew's hand-over waiting for its own refusal.
    let rows = w.core.store.crew_deliveries(&task).unwrap();
    let modes: Vec<&str> = rows
        .iter()
        .filter(|row| row.message_id == message_id)
        .map(|row| row.mode.as_str())
        .collect();
    assert_eq!(modes, ["live"], "{rows:?}");
    let waiting = w.core.crew_bus.live_steers.waiting();
    let refused = one(&waiting, "crew hand-over waiting for its own refusal").clone();

    // The crew message's own refusal still matches and is the one that corrects its record.
    w.core.crew_steer_failed(
        &worker,
        &SteerMessage {
            message_id: refused,
            prompt: text,
        },
    );
    let rows = w.core.store.crew_deliveries(&task).unwrap();
    let modes: Vec<&str> = rows
        .iter()
        .filter(|row| row.message_id == message_id)
        .map(|row| row.mode.as_str())
        .collect();
    assert_eq!(modes, ["live", "next_turn"], "{rows:?}");
}

/// #444: a steer the crew bus never handed over writes no delivery row when the engine refuses it:
/// the bus never guesses which message a person's own refused steer belonged to.
#[tokio::test]
async fn a_refused_steer_the_bus_did_not_hand_over_writes_no_row() {
    let w = world_refusing_steers(true, Arc::new(|_, _| Turn::Hang), &[("alpha", true)]).await;
    let (started, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{started}");
    let task = started["task"].as_str().unwrap().to_owned();
    let crew = w.task(&task).crew_id;
    let worker = w.worker(&task);
    w.until("the worker's turn runs", |w| running(w, &worker))
        .await;

    // The person's own steer, not a crew message: the engine refuses it and the core requeues it.
    w.core
        .runs
        .steer(&worker, None, "also check the docs")
        .unwrap();
    w.until("the person's refused steer is requeued", |w| {
        queue(w, &worker)
            .iter()
            .any(|prompt| prompt == "also check the docs")
    })
    .await;
    assert!(
        w.core
            .store
            .crew_deliveries_in_crew(&crew)
            .unwrap()
            .is_empty(),
        "a refusal of a steer the bus never handed over wrote a delivery row"
    );
}
