//! A flood of held messages or refused writes cannot cost a worker its retry prompt, the lead its
//! result notice, or the store its room.

use orbyn_contracts::{CrewRole, ResultState};
use serde_json::json;

use super::bus_support::{Duo, duo};

/// A body of about 4000 characters, different for each `n`.
fn big(n: u32) -> String {
    format!("{n} {}", "x".repeat(3990))
}

/// Held messages are capped per prompt, are recorded as sent only when the prompt is accepted,
/// and never take the core's own prompt down with them.
#[tokio::test]
async fn a_flood_of_held_messages_costs_no_prompt() {
    let d = duo().await;
    let (wa, wb) = (d.w.worker(&d.a), d.w.worker(&d.b));
    // A holds 14 messages for the lead (idle); the lead holds 14 for B (its engine takes no input).
    // (The lead's own calls would collect what is held for it, so its messages go first.)
    for n in 0..14 {
        let (sent, error) = d
            .call(
                &d.w.lead_token,
                "crew_post",
                Duo::fact(&d.mb, &big(100 + n)),
            )
            .await;
        assert!(!error && sent["delivered"][0]["mode"] == "held", "{sent}");
    }
    for n in 0..14 {
        let (sent, error) = d.call(&d.ta, "crew_post", Duo::fact("lead", &big(n))).await;
        assert!(!error, "{sent}");
    }
    let crew = d.w.task(&d.a).crew_id;
    let held = |who: &str| d.w.core.store.held_crew_messages(&crew, who).unwrap().len();
    assert_eq!((held("lead"), held(&d.mb)), (14, 14));

    // The lead's result notice goes out with at most ten of them and says there are more.
    let task = d.w.task(&d.a);
    d.w.core.end_crew_task(
        &task,
        ResultState::Cancelled,
        None,
        "stopped for the test",
        None,
    );
    d.w.until("the lead is sent the result", |w| {
        w.sent_to(&w.lead)
            .iter()
            .any(|p| p.contains("<crew-result"))
    })
    .await;
    let prompt =
        d.w.sent_to(&d.w.lead)
            .into_iter()
            .find(|p| p.contains("<crew-result"))
            .unwrap();
    assert!(
        prompt.len() < 30_000 && prompt.contains("more crew messages are held"),
        "{}",
        prompt.len()
    );
    let left = held("lead");
    assert!((1..14).contains(&left), "the rest stay held: {left}");

    // B's retry prompt carries ten and leaves four.
    let (task, _) = (d.w.task(&d.b), ());
    d.w.core.send_to_worker(&task, "retry now").unwrap();
    let queued = d.queued(&wb);
    assert_eq!(queued.len(), 1);
    assert!(
        queued[0].starts_with("retry now") && queued[0].contains("more crew messages are held")
    );
    assert!(queued[0].len() < 30_000);
    let left_b = held(&d.mb);
    assert!((1..14).contains(&left_b), "{left_b}");

    // A prompt the thread refuses with the held text goes alone, and they stay held.
    let base = "y".repeat(119_990);
    d.w.core.send_to_worker(&task, &base).unwrap();
    let queued = d.queued(&wb);
    assert_eq!(queued.len(), 2);
    assert_eq!(queued[1], base);
    assert_eq!(
        held(&d.mb),
        left_b,
        "a refused prompt marks nothing as sent"
    );

    // The tool-result inbox is capped the same way.
    let (read, _) = d
        .call(&d.tb, "crew_board_read", json!({ "key": "none" }))
        .await;
    assert!(read["inbox"].as_array().unwrap().len() <= 11);
    let _ = wa;
}

/// Stale board writes are not stored, so they cost no post; the lead is told of a conflict once
/// per key and writer, and of at most ten things a run.
#[tokio::test]
async fn stale_writes_cannot_flood_the_lead_with_notices() {
    let d = duo().await;
    let stale = |key: &str, base: u64| json!({ "key": key, "class": "decision", "value": 1, "base_rev": base });
    for base in 1..6 {
        assert_eq!(
            d.refused(&d.ta, "crew_board_write", stale("same", base))
                .await,
            "board_conflict"
        );
    }
    let notices = |d: &Duo| {
        d.w.core
            .store
            .crew_messages(&d.a)
            .unwrap()
            .into_iter()
            .filter(|m| m.author.role == CrewRole::Core)
            .count()
    };
    assert_eq!(notices(&d), 1, "one conflict, one notice");
    for n in 0..25 {
        d.refused(&d.ta, "crew_board_write", stale(&format!("key.{n}"), 7))
            .await;
    }
    assert_eq!(notices(&d), 10);
    assert_eq!(d.w.core.store.count_crew_posts(&d.a, &d.ma).unwrap(), 0);
}
