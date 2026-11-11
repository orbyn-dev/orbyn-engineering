//! The bus on two fake engines: a board fact crosses between running turns, the hop limit
//! trips, a question waits for its answer, and nothing a member sends is taken once its run has
//! ended.

use orbyn_contracts::{CrewObject, MessageKind};
use serde_json::{Value, json};

use super::bus_support::{Duo, duo};
use super::harness::{brief, key};

fn shape(value: &str, base_rev: u64) -> Value {
    json!({ "key": "api.shape", "class": "decision", "value": { "route": value }, "base_rev": base_rev })
}

/// Slice 4, done when (1): two engines exchange a board fact in the middle of their turns.
#[tokio::test]
async fn two_engines_exchange_a_board_fact_mid_turn() {
    let d = duo().await;
    let (wa, wb) = (d.w.worker(&d.a), d.w.worker(&d.b));

    // A decides the shape, puts it on the board, and tells B. `beta` takes no input in a turn,
    // so the message waits for B's next one: the record says so and does not say `live`.
    let (wrote, error) = d.call(&d.ta, "crew_board_write", shape("/parse", 0)).await;
    assert!(!error, "{wrote}");
    assert_eq!(wrote["rev"], 1);
    let (sent, error) = d
        .call(
            &d.ta,
            "crew_post",
            Duo::fact(&d.mb, "api.shape is on the board"),
        )
        .await;
    assert!(!error, "{sent}");
    assert_eq!(sent["delivered"][0]["to"], json!(d.mb));
    assert_eq!(sent["delivered"][0]["mode"], "next_turn");
    let waiting = d.queued(&wb);
    assert_eq!(waiting.len(), 1, "{waiting:?}");
    assert!(waiting[0].contains("trust=\"data\""), "{}", waiting[0]);
    assert!(waiting[0].contains("api.shape is on the board"));
    assert!(d.steered_to(&wb).is_empty());

    // B reads what A wrote, with its author and revision, and builds on it.
    let (read, error) = d
        .call(&d.tb, "crew_board_read", json!({ "key": "api.shape" }))
        .await;
    assert!(!error, "{read}");
    assert_eq!(read["fact"]["value"]["route"], "/parse");
    assert_eq!(read["fact"]["rev"], 1);
    assert_eq!(read["fact"]["author"]["member"], json!(d.ma));
    let (built, error) = d
        .call(&d.tb, "crew_board_write", shape("/parse/duration", 1))
        .await;
    assert!(!error, "{built}");
    assert_eq!(built["rev"], 2);

    // B tells A. `alpha` takes input mid-turn, so this one is handed to the running turn.
    let (sent, error) = d
        .call(
            &d.tb,
            "crew_post",
            Duo::fact(&d.ma, "I moved it to /parse/duration"),
        )
        .await;
    assert!(!error, "{sent}");
    assert_eq!(sent["delivered"][0]["mode"], "live");
    d.w.until("the turn is handed the text", |_| {
        !d.steered_to(&wa).is_empty()
    })
    .await;
    let text = &d.steered_to(&wa)[0];
    assert!(
        text.contains(&format!("from=\"{}\"", d.mb)) && text.contains("trust=\"data\""),
        "{text}"
    );

    // A wrote on revision 1 after B moved it to 2: refused, the first value stands, and the
    // lead is told.
    assert_eq!(
        d.refused(&d.ta, "crew_board_write", shape("/other", 1))
            .await,
        "board_conflict"
    );
    let (now, _) = d
        .call(&d.ta, "crew_board_read", json!({ "key": "api.shape" }))
        .await;
    assert_eq!(now["fact"]["value"]["route"], "/parse/duration");
    d.w.until("the lead is told", |w| {
        w.sent_to(&w.lead)
            .iter()
            .any(|p| p.contains("BoardConflict"))
    })
    .await;

    // How each message got there is on record, per recipient; the messages hold no mode.
    let by_a = d.w.core.store.crew_deliveries(&d.a).unwrap();
    assert_eq!(
        (by_a[0].recipient.as_str(), by_a[0].mode.as_str()),
        (d.mb.as_str(), "next_turn")
    );
    let by_b = d.w.core.store.crew_deliveries(&d.b).unwrap();
    assert_eq!(
        (by_b[0].recipient.as_str(), by_b[0].mode.as_str()),
        (d.ma.as_str(), "live")
    );

    // What B may read is the brief, what was sent to it and the board: not what A sent to others.
    let seen = d.w.core.store.crew_worker_objects(&d.b, &d.mb).unwrap();
    let kinds: Vec<String> = seen
        .iter()
        .map(|o| format!("{:?}", CrewObject::from(o.clone()).type_name()))
        .collect();
    assert_eq!(kinds, ["\"brief\"", "\"message\"", "\"fact\""], "{kinds:?}");
}

/// Slice 4, done when (2): the hop limit trips, counts both ways, and a lead message to one
/// member re-arms the pairs that member is in.
#[tokio::test]
async fn the_hop_limit_trips_and_the_lead_resets_the_pair() {
    let d = duo().await;
    let say =
        |from: &str, to: &str, n: u32| (from.to_owned(), Duo::fact(to, &format!("message {n}")));

    for (n, (from, to)) in [(&d.ta, &d.mb), (&d.tb, &d.ma), (&d.ta, &d.mb)]
        .into_iter()
        .enumerate()
    {
        let (token, args) = say(from, to, n as u32);
        let (answer, error) = d.call(&token, "crew_post", args).await;
        assert!(!error, "{answer}");
    }
    // The fourth message of the pair is refused, whichever way it goes.
    let (token, args) = say(&d.tb, &d.ma, 3);
    assert_eq!(
        d.refused(&token, "crew_post", args).await,
        "hop_limit_exceeded"
    );
    let (token, args) = say(&d.ta, &d.mb, 4);
    assert_eq!(
        d.refused(&token, "crew_post", args).await,
        "hop_limit_exceeded"
    );
    d.w.until("the lead is told", |w| {
        w.sent_to(&w.lead)
            .iter()
            .any(|p| p.contains("reached 3 messages"))
    })
    .await;

    // To the lead and to everyone do not count against the pair.
    for n in 0..4 {
        for to in ["lead", "all"] {
            let (answer, error) = d
                .call(
                    &d.ta,
                    "crew_post",
                    Duo::fact(to, &format!("update {n} {to}")),
                )
                .await;
            assert!(!error, "{answer}");
        }
    }
    // A lead message to everyone resets nothing.
    let (all, error) = d
        .call(&d.w.lead_token, "crew_post", Duo::fact("all", "carry on"))
        .await;
    assert!(!error, "{all}");
    let (token, args) = say(&d.tb, &d.ma, 5);
    assert_eq!(
        d.refused(&token, "crew_post", args).await,
        "hop_limit_exceeded"
    );

    // The lead steps in with A: the pair counts again from zero, and trips again at three.
    let (lead, error) = d
        .call(
            &d.w.lead_token,
            "crew_post",
            Duo::fact(&d.ma, "I read the thread"),
        )
        .await;
    assert!(!error, "{lead}");
    for n in 6..9 {
        let (token, args) = say(&d.tb, &d.ma, n);
        let (answer, error) = d.call(&token, "crew_post", args).await;
        assert!(!error, "{answer}");
    }
    let (token, args) = say(&d.ta, &d.mb, 9);
    assert_eq!(
        d.refused(&token, "crew_post", args).await,
        "hop_limit_exceeded"
    );
}

/// Slice 4, done when (3): once a run has ended, every member tool refuses, a message to that
/// member is refused too, and the core can still add Evidence and a Gate (H7).
#[tokio::test]
async fn a_message_after_the_run_ends_is_refused() {
    let d = duo().await;
    d.w.core
        .store
        .move_crew_task(&d.a, "cancelled", None)
        .unwrap();
    let calls = [
        ("crew_post", Duo::fact("lead", "still here")),
        (
            "crew_ask",
            json!({ "to": "lead", "body": "anyone?", "wait_seconds": 0 }),
        ),
        ("crew_board_read", json!({ "key": "api.shape" })),
        ("crew_board_write", shape("/late", 0)),
    ];
    for (tool, args) in calls {
        assert_eq!(d.refused(&d.ta, tool, args).await, "run_ended", "{tool}");
    }
    // Nobody can send to the ended member either: not a worker, not the lead.
    assert_eq!(
        d.refused(&d.tb, "crew_post", Duo::fact(&d.ma, "too late"))
            .await,
        "run_ended"
    );
    assert_eq!(
        d.refused(&d.w.lead_token, "crew_post", Duo::fact(&d.ma, "too late"))
            .await,
        "run_ended"
    );
    assert!(d.queued(&d.w.worker(&d.a)).is_empty());
    assert_eq!(d.w.core.store.count_crew_posts(&d.a, &d.ma).unwrap(), 0);

    // The other run is live, and a lead message to everyone still reaches it.
    let (answer, error) = d
        .call(&d.w.lead_token, "crew_post", Duo::fact("all", "B only"))
        .await;
    assert!(!error, "{answer}");
    assert_eq!(answer["delivered"].as_array().unwrap().len(), 1);

    // The core's own tools keep working on the ended run: a check runs again, a gate is added.
    let task = d.w.task(&d.a);
    for (name, example) in [
        (
            "evidence",
            include_str!(
                "../../../../../docs/crew/protocol/schemas/examples/evidence.valid.test-pass.json"
            ),
        ),
        (
            "gate",
            include_str!(
                "../../../../../docs/crew/protocol/schemas/examples/gate.valid.checks-by-code.json"
            ),
        ),
    ] {
        let mut value: Value = serde_json::from_str(example).unwrap();
        value["crew_id"] = json!(task.crew_id);
        value["run_id"] = json!(task.run_id);
        let mut object: CrewObject = serde_json::from_value(value).unwrap();
        let id = object.computed_id().unwrap();
        match &mut object {
            CrewObject::Evidence(o) => o.id = id,
            CrewObject::Gate(o) => o.id = id,
            _ => unreachable!(),
        }
        d.w.core
            .store
            .add_crew_object(&d.a, &object)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
    }
    assert_eq!(d.w.core.store.crew_gates(&d.a).unwrap().len(), 1);
}

/// `crew_ask` returns `pending` when nothing answers in time, and the answer arrives later as a
/// message. An answer given while the asker waits comes back to that `crew_ask` instead.
#[tokio::test]
async fn an_ask_goes_pending_or_returns_the_answer() {
    let d = duo().await;
    let wa = d.w.worker(&d.a);

    let (pending, error) = d
        .call(
            &d.ta,
            "crew_ask",
            json!({ "to": d.mb, "body": "Which route?", "wait_seconds": 0 }),
        )
        .await;
    assert!(!error, "{pending}");
    assert_eq!(pending["state"], "pending");
    let question = pending["question"].clone();
    // The question reached B as data, with how to answer it.
    let queued = d.queued(&d.w.worker(&d.b));
    assert!(
        queued[0].contains("kind=\"question\"") && queued[0].contains("in_reply_to"),
        "{queued:?}"
    );

    // The late answer is an ordinary message to A: handed to its running turn.
    let (late, error) = d
        .call(
            &d.tb,
            "crew_post",
            json!({ "to": d.ma, "kind": "answer", "body": "/parse", "in_reply_to": question }),
        )
        .await;
    assert!(!error, "{late}");
    assert_eq!(late["delivered"][0]["mode"], "live");
    d.w.until("A is handed the answer", |_| d.steered_to(&wa).len() == 1)
        .await;

    // A waiting ask gets its answer back itself, and the answer is not also sent to the turn.
    let (lead, error) = d
        .call(&d.w.lead_token, "crew_post", Duo::fact(&d.ma, "carry on"))
        .await;
    assert!(!error, "{lead}");
    d.w.until("A is handed the lead's message", |_| {
        d.steered_to(&wa).len() == 2
    })
    .await;
    let crew = d.w.task(&d.a).crew_id;
    let ask = d.call(
        &d.ta,
        "crew_ask",
        json!({ "to": d.mb, "body": "Second try?", "wait_seconds": 10 }),
    );
    let answerer = async {
        let id = loop {
            let log = d.w.core.store.crew_messages_in_crew(&crew).unwrap();
            if let Some((_, q)) = log.iter().find(|(_, m)| m.body.as_str() == "Second try?") {
                break q.id.clone();
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        };
        d.call(
            &d.tb,
            "crew_post",
            json!({ "to": d.ma, "kind": "answer", "body": "yes", "in_reply_to": id }),
        )
        .await
    };
    let ((asked, ask_error), (given, give_error)) = tokio::join!(ask, answerer);
    assert!(!ask_error && !give_error, "{asked} {given}");
    assert_eq!(asked["state"], "answered");
    assert_eq!(asked["answer"]["body"], "yes");
    assert!(
        asked["delivered_as"]
            .as_str()
            .unwrap()
            .contains("trust=\"data\"")
    );
    assert_eq!(
        d.steered_to(&wa).len(),
        2,
        "the answer came back through the ask only"
    );

    let kinds: Vec<MessageKind> =
        d.w.core
            .store
            .crew_messages_in_crew(&crew)
            .unwrap()
            .iter()
            .map(|(_, m)| m.kind)
            .collect();
    assert!(kinds.contains(&MessageKind::Question) && kinds.contains(&MessageKind::Answer));
}

/// H10, expire on use: a lease that has run out is recorded as expired the first time anyone
/// looks and then blocks nobody, so its files go to another writer. Its holder is still a member
/// of a live run and keeps the bus; leases and messages are separate rules.
#[tokio::test]
async fn an_expired_lease_blocks_no_other_member() {
    let d = duo().await;
    let crew = d.w.task(&d.a).crew_id;
    let before =
        d.w.start(brief(json!({ "outcome": "Same files." })), key())
            .await;
    assert_eq!(before.0["code"], "lease_conflict", "{}", before.0);

    let later = crate::util::unix_now() + 3 * 3600;
    assert!(d.w.core.live_crew_leases(&crew, later).unwrap().is_empty());
    let (third, error) =
        d.w.start(brief(json!({ "outcome": "Same files, later." })), key())
            .await;
    assert!(!error, "{third}");
    let (sent, error) = d
        .call(&d.ta, "crew_post", Duo::fact("lead", "still on it"))
        .await;
    assert!(!error, "{sent}");
}
