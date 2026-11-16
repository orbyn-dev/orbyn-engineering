//! What the bus must not do: start a turn or write a person's queue, be a way round the post
//! limit, or show text the answer key now covers. A key is added after the text was stored by
//! starting a third task whose check command is in it. The needle is named by label.

use orbyn_contracts::AgUiEvent;
use serde_json::{Value, json};

use super::bus_support::{Duo, duo};
use super::harness::{brief, key};
use crate::crew::bus::deliver::Target;

const NEEDLE: &str = "orbyn-late-needle-9";

fn alive(text: &str) -> bool {
    text.contains(NEEDLE)
}

/// A third task whose check command holds `NEEDLE`, so the crew's keys now cover it.
async fn add_late_key(d: &Duo) {
    let mut late = key();
    late["checks"][1]["params"]["run"] = json!(format!("echo {NEEDLE}"));
    let (started, error) =
        d.w.start(
            brief(json!({
                "outcome": "Third.",
                "files": [{ "path": "lib/**", "mode": "write" }],
            })),
            late,
        )
        .await;
    assert!(!error, "{started}");
}

fn roles(d: &Duo, thread: &str) -> Vec<(String, String)> {
    let mut lines = Vec::new();
    for event in d.w.core.store.events_after(thread, 0, 500).unwrap() {
        if let AgUiEvent::TextMessageStart { role, message_id } = event.event {
            lines.push((message_id, role));
        }
    }
    lines
}

/// The bus hands text only to a running turn. It starts no turn, writes no queue, and what it
/// steers shows under the crew's role, never the person's. A worker being verified is not
/// handed anything, whatever its thread is doing.
#[tokio::test]
async fn a_crew_message_starts_no_turn_and_is_never_the_persons_input() {
    let d = duo().await;
    let (wa, wb) = (d.w.worker(&d.a), d.w.worker(&d.b));
    let before = roles(&d, &wa).len();

    // To the lead (idle) and to the worker whose engine takes no input: held, nothing runs.
    let (to_lead, _) = d
        .call(&d.ta, "crew_post", Duo::fact("lead", "for the lead"))
        .await;
    assert_eq!(to_lead["delivered"][0]["mode"], "held");
    let (to_b, _) = d.call(&d.ta, "crew_post", Duo::fact(&d.mb, "for B")).await;
    assert_eq!(to_b["delivered"][0]["mode"], "held");
    // A refusal told to the lead takes the same road.
    assert_eq!(
        d.refused(
            &d.ta,
            "crew_board_write",
            json!({ "key": "k", "class": "decision", "value": 1, "base_rev": 4 })
        )
        .await,
        "board_conflict"
    );
    assert!(
        d.w.sent_to(&d.w.lead).is_empty(),
        "no lead turn was started"
    );
    for thread in [&d.w.lead, &wa, &wb] {
        assert!(
            d.queued(thread).is_empty(),
            "the person's queue stays the person's"
        );
    }

    // A running turn that takes input is steered, under the crew's role.
    let (to_a, _) = d.call(&d.tb, "crew_post", Duo::fact(&d.ma, "for A")).await;
    assert_eq!(to_a["delivered"][0]["mode"], "live");
    let seen = roles(&d, &wa);
    assert_eq!(seen.len(), before + 1);
    // The worker's own first prompt is the lead's brief; what the bus added is the crew's.
    assert_eq!(seen[before].1, "crew", "{seen:?}");

    // A's task moves on to verifying while its turn still takes input: the state is read when the
    // text is handed over, so nothing is handed over. It waits, and no turn starts.
    d.w.core
        .store
        .move_crew_task(&d.a, "verifying", None)
        .unwrap();
    let steered = d.steered_to(&wa).len();
    let (late, _) = d
        .call(
            &d.tb,
            "crew_post",
            Duo::fact(&d.ma, "too late for the turn"),
        )
        .await;
    assert_eq!(late["delivered"][0]["mode"], "held");
    assert_eq!(d.steered_to(&wa).len(), steered);
    assert!(d.queued(&wa).is_empty());

    // The recipient reads what is held in the result of its next bus tool call, and only once.
    let (read, _) = d
        .call(&d.ta, "crew_board_read", json!({ "key": "none" }))
        .await;
    let inbox = read["inbox"].as_array().unwrap();
    assert!(
        inbox[0].as_str().unwrap().contains("too late for the turn"),
        "{read}"
    );
    let (again, _) = d
        .call(&d.ta, "crew_board_read", json!({ "key": "none" }))
        .await;
    assert!(again.get("inbox").is_none());
    let modes: Vec<String> =
        d.w.core
            .store
            .crew_deliveries(&d.b)
            .unwrap()
            .into_iter()
            .map(|r| r.mode)
            .collect();
    assert_eq!(modes, ["live", "held", "tool_result"]);

    // The lead reads its notices the same way.
    let (lead, _) = d
        .call(&d.w.lead_token, "crew_board_read", json!({ "key": "none" }))
        .await;
    assert_eq!(lead["inbox"].as_array().unwrap().len(), 2, "{lead}");
}

/// H5 and the board: every write is a post, and a key has a fixed number of revisions, so two
/// workers cannot talk without limit by rewriting one key.
#[tokio::test]
async fn board_writes_count_as_posts_and_a_key_has_a_revision_cap() {
    let d = duo().await;
    d.w.core.crew_bus.lower(None, Some(3));
    let write =
        |rev: u64| json!({ "key": "k", "class": "observation", "value": rev, "base_rev": rev });
    for rev in 0..3 {
        let (done, error) = d.call(&d.ta, "crew_board_write", write(rev)).await;
        assert!(!error, "{done}");
    }
    assert_eq!(
        d.refused(&d.ta, "crew_board_write", write(3)).await,
        "post_limit_exceeded"
    );
    assert_eq!(
        d.refused(&d.ta, "crew_post", Duo::fact("lead", "one more"))
            .await,
        "post_limit_exceeded"
    );
    assert!(
        d.lead_notices()
            .iter()
            .any(|n| n.contains("reached 3 posts"))
    );
    // The lead is not counted, and a refused write was not stored.
    assert_eq!(
        d.w.core
            .store
            .crew_fact(&d.w.task(&d.a).crew_id, "k")
            .unwrap()
            .unwrap()
            .rev,
        3
    );

    // Two workers rewriting one key hit the revision cap, not the exchange limit.
    let d = duo().await;
    for rev in 0..16u64 {
        let token = if rev % 2 == 0 { &d.ta } else { &d.tb };
        let (done, error) = d.call(token, "crew_board_write", write(rev)).await;
        assert!(!error, "{rev}: {done}");
    }
    assert_eq!(
        d.refused(&d.ta, "crew_board_write", write(16)).await,
        "invalid_object"
    );
    assert!(d.lead_notices().iter().any(|n| n.contains("16 revisions")));
}

/// H1 and H2 on the way out: text stored before a key existed is checked again when it leaves,
/// on every road: a live steer, a prompt, a tool result, a board read and an ask's answer.
#[tokio::test]
async fn text_stored_before_a_key_is_checked_again_when_it_leaves() {
    let d = duo().await;
    let (wa, crew) = (d.w.worker(&d.a), d.w.task(&d.a).crew_id);
    let body = format!("I ran echo {NEEDLE} and it passed");
    let (live, error) = d.call(&d.tb, "crew_post", Duo::fact(&d.ma, &body)).await;
    assert!(!error && live["delivered"][0]["mode"] == "live", "{live}");
    // A's task is verifying from here on, so what B sends A is held.
    d.w.core
        .store
        .move_crew_task(&d.a, "verifying", None)
        .unwrap();
    let (for_a, _) = d
        .call(
            &d.tb,
            "crew_post",
            Duo::fact(&d.ma, &format!("{body} for A")),
        )
        .await;
    assert_eq!(for_a["delivered"][0]["mode"], "held");
    let (held_one, _) = d.call(&d.ta, "crew_post", Duo::fact(&d.mb, &body)).await;
    let (held_two, _) = d
        .call(
            &d.ta,
            "crew_post",
            Duo::fact(&d.mb, &format!("{body} again")),
        )
        .await;
    let (fact, error) = d
        .call(
            &d.ta,
            "crew_board_write",
            json!({ "key": "ran", "class": "observation", "value": body, "base_rev": 0 }),
        )
        .await;
    assert!(!error, "{fact}");
    // Not a worker, but a question and its answer, stored while the text was allowed.
    let (asked, _) = d
        .call(
            &d.ta,
            "crew_ask",
            json!({ "to": "lead", "body": "ok?", "wait_seconds": 0 }),
        )
        .await;
    let (answered, error) = d
        .call(
            &d.w.lead_token,
            "crew_post",
            json!({ "to": d.ma, "kind": "answer", "body": body, "in_reply_to": asked["question"] }),
        )
        .await;
    assert!(!error, "{answered}");

    add_late_key(&d).await;
    let stored = d.w.core.store.crew_messages_in_crew(&crew).unwrap();
    let message = |id: &Value| {
        stored
            .iter()
            .find(|(_, m)| json!(m.id) == *id)
            .unwrap()
            .1
            .clone()
    };
    let needles = d.w.core.crew_key_needles(&crew).unwrap();
    assert!(needles.iter().any(|n| alive(n)), "the late key is in force");

    // A live steer: the turn is handed a plain notice, not the text.
    let target = Target {
        recipient: d.ma.clone(),
        thread: Some(wa.clone()),
        // No task to read: this call is about the text check, and A's task is verifying now.
        task: None,
        guard: true,
    };
    d.w.until("the first steer arrives", |_| !d.steered_to(&wa).is_empty())
        .await;
    let steered = d.steered_to(&wa).len();
    let mode =
        d.w.core
            .deliver_crew_message(&message(&live["id"]), &target, &needles);
    assert_eq!(mode, "live");
    d.w.until("the second steer arrives", |_| {
        d.steered_to(&wa).len() > steered
    })
    .await;
    let handed = d.steered_to(&wa).remove(steered);
    assert!(
        !alive(&handed) && handed.contains("withheld"),
        "the live steer leaked"
    );

    // A held message sent with the next prompt, and one returned in a tool result.
    let prompt = d.held_text(&d.b);
    assert!(
        prompt.contains("withheld") && !alive(&prompt),
        "the prompt leaked"
    );
    let (read, _) = d
        .call(&d.ta, "crew_board_read", json!({ "key": "none" }))
        .await;
    let inbox = read["inbox"].to_string();
    assert!(
        inbox.contains("withheld") && !alive(&inbox),
        "the tool result leaked"
    );
    let _ = (held_one, held_two);

    // The board read refuses a worker the fact; the lead still reads it.
    assert_eq!(
        d.refused(&d.tb, "crew_board_read", json!({ "key": "ran" }))
            .await,
        "answer_key_forbidden"
    );
    let (lead, error) = d
        .call(&d.w.lead_token, "crew_board_read", json!({ "key": "ran" }))
        .await;
    assert!(!error && lead["fact"]["rev"] == 1);

    // An ask returns an answer that the keys now cover as a notice.
    let answer = message(&answered["id"]);
    let view =
        d.w.core
            .answered_view(asked["question"].as_str().unwrap(), &answer, true);
    assert_eq!(view["withheld"], true);
    assert!(!alive(&view.to_string()), "the ask's answer leaked");
    assert!(
        d.w.core.answered_view("q", &answer, false)["answer"].is_object(),
        "the lead's own is not checked"
    );
}
