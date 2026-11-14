//! The Map, message and fact reads on two fake engines (`alpha` is steered mid-turn, `beta` is not): the nodes, edges and
//! delivery states are right, every type has exactly the fields it should, the reads change nothing, the flag off hides them, and
//! nothing a member wrote (a message body, a fact value) reaches any read of the crew. No test depends on the clock.

use orbyn_contracts::{FlowDeliveryMode, FlowRole};
use serde_json::{Value, json};

use super::bus_support::{Duo, duo};
use super::flow::{crew_rows, nonce, page};
use super::harness::{Turn, world};
use super::review_support::arc;

fn keys(value: &impl serde::Serialize) -> Vec<String> {
    let mut keys: Vec<String> = serde_json::to_value(value)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

fn shape(value: &str, base_rev: u64) -> Value {
    json!({ "key": "api.shape", "class": "decision", "value": { "route": value }, "base_rev": base_rev })
}

/// Two workers who have written to each other, with a runtime marker in every text they wrote.
async fn talked(marker: &str) -> (Duo, String) {
    let d = duo().await;
    let (_, error) = d.call(&d.ta, "crew_board_write", shape(marker, 0)).await;
    assert!(!error, "the first fact was refused");
    let (_, error) = d.call(&d.tb, "crew_board_write", shape(marker, 1)).await;
    assert!(!error, "the second fact was refused");
    // beta's message to alpha is handed to alpha's running turn; alpha's to beta waits, and beta makes no call after it.
    let (_, error) = d
        .call(
            &d.tb,
            "crew_post",
            Duo::fact(&d.ma, &format!("reply {marker}")),
        )
        .await;
    assert!(!error, "the first message was refused");
    let (_, error) = d
        .call(
            &d.ta,
            "crew_post",
            Duo::fact(&d.mb, &format!("note {marker}")),
        )
        .await;
    assert!(!error, "the second message was refused");
    let crew = d.w.task(&d.a).crew_id;
    (d, crew)
}

#[tokio::test]
async fn the_map_has_a_lead_and_the_workers_and_says_how_each_message_was_delivered() {
    let marker = nonce("map");
    let (d, crew) = talked(&marker).await;
    let before = crew_rows(&d.w);
    let map = d.w.core.crew_flow_map(&crew).await.unwrap();

    assert_eq!(map.nodes.len(), 3, "the lead and two workers");
    assert_eq!(map.nodes[0].role, FlowRole::Lead);
    let workers: Vec<_> = map
        .nodes
        .iter()
        .filter(|n| n.role == FlowRole::Worker)
        .collect();
    assert_eq!(workers.len(), 2);
    for node in &workers {
        assert_eq!(
            node.started_by.as_deref(),
            Some(map.nodes[0].member.as_str())
        );
        assert!(node.task.is_some() && node.state.as_deref() == Some("running"));
        assert!(!node.outcome.is_empty() && node.lease.len() <= 3);
    }
    let alpha = workers.iter().find(|n| n.engine == "alpha").unwrap();
    let beta = workers.iter().find(|n| n.engine == "beta").unwrap();
    assert_eq!(alpha.unread, 0, "alpha was steered live");
    assert_eq!(beta.unread, 1, "one message waits for beta's next turn");
    assert!(!beta.read_only);
    assert_eq!(map.engines.len(), 2);

    assert_eq!(map.edges.len(), 2);
    let to_beta = map.edges.iter().find(|e| e.to == beta.member).unwrap();
    assert_eq!(
        to_beta.delivery,
        Some(FlowDeliveryMode::Held),
        "beta takes no input mid-turn"
    );
    assert_eq!(to_beta.label, "a fact");
    let to_alpha = map.edges.iter().find(|e| e.to == alpha.member).unwrap();
    assert_eq!(to_alpha.delivery, Some(FlowDeliveryMode::Live));
    assert!(!map.truncated);

    // The fields are exactly these: a field added to a type fails here until it is reviewed.
    assert_eq!(
        keys(&map.nodes[1]),
        [
            "created_at",
            "effort",
            "engine",
            "engine_status",
            "lease",
            "member",
            "model",
            "outcome",
            "quiet",
            "read_only",
            "role",
            "started_by",
            "state",
            "subagents",
            "task",
            "unread",
            "updated_at"
        ]
    );
    assert_eq!(
        keys(&map.edges[0]),
        ["count", "delivery", "from", "kind", "label", "to"]
    );
    assert_eq!(keys(&map.engines[0]), ["engine", "status"]);
    assert_eq!(
        keys(&map),
        ["crew", "edges", "engines", "nodes", "truncated"]
    );

    // Reading changed no row of any crew table.
    for _ in 0..3 {
        d.w.core.crew_flow_map(&crew).await.unwrap();
    }
    assert_eq!(before, crew_rows(&d.w));
}

#[tokio::test]
async fn messages_and_facts_are_read_by_labels_and_never_by_their_words() {
    let marker = nonce("words");
    let (d, crew) = talked(&marker).await;
    let list = d.w.core.crew_flow_messages(&crew, None, page()).unwrap();
    assert_eq!(
        (list.messages.len(), list.total, list.next_offset),
        (2, 2, None)
    );
    assert_eq!(list.messages[0].from, d.ma, "newest first");
    assert_eq!(list.messages[0].delivery, Some(FlowDeliveryMode::Held));
    assert_eq!(list.messages[1].delivery, Some(FlowDeliveryMode::Live));
    assert!(
        list.messages
            .iter()
            .all(|m| m.label == "a fact" && m.kind == "fact")
    );
    assert!(
        list.messages
            .iter()
            .all(|m| !m.at.is_empty() && m.delivered_at.is_some())
    );
    assert_eq!(
        keys(&list.messages[0]),
        [
            "at",
            "delivered_at",
            "delivery",
            "from",
            "id",
            "in_reply_to",
            "kind",
            "label",
            "to"
        ]
    );
    assert_eq!(keys(&list), ["messages", "next_offset", "total"]);

    // A member filter keeps what the member sent or was sent; a page of one says where the next starts.
    let mine =
        d.w.core
            .crew_flow_messages(&crew, Some(&d.ma), page())
            .unwrap();
    assert_eq!(mine.total, 2);
    let one =
        d.w.core
            .crew_flow_messages(&crew, None, crate::crew::Page::new(Some(1), None).unwrap())
            .unwrap();
    assert_eq!((one.messages.len(), one.next_offset), (1, Some(1)));
    assert!(
        d.w.core
            .crew_flow_messages(&crew, Some("../x"), page())
            .is_err()
    );

    let facts = d.w.core.crew_flow_facts(&crew).unwrap();
    assert_eq!(
        (facts.facts.len(), facts.total, facts.truncated),
        (1, 1, false)
    );
    let fact = &facts.facts[0];
    assert_eq!(
        (fact.key.as_str(), fact.class.as_str(), fact.rev),
        ("api.shape", "decision", 2)
    );
    assert_eq!(fact.author, d.mb);
    assert!(!fact.verified, "it names no evidence");
    assert_eq!(
        keys(fact),
        ["at", "author", "class", "key", "rev", "verified"]
    );
    assert_eq!(keys(&facts), ["facts", "total", "truncated"]);

    // Nothing a member wrote is in any read of the crew: not in the map, the messages, the facts or the task's zoom.
    let map = d.w.core.crew_flow_map(&crew).await.unwrap();
    // The zoom is for a work task; a worker of this crew that is one reads, a reviewer's task is refused.
    let zoom = serde_json::to_string(&d.w.core.crew_flow_task(&d.a).unwrap()).unwrap();
    let everything = [
        serde_json::to_string(&map).unwrap(),
        serde_json::to_string(&list).unwrap(),
        serde_json::to_string(&facts).unwrap(),
        zoom,
    ]
    .join(" ");
    assert!(
        !everything.contains(&marker),
        "a member's words reached a read"
    );
    for field in ["thread", "worktree", "body"] {
        assert!(!everything.contains(field), "a read names {field}");
    }
}

#[tokio::test]
async fn the_flag_off_hides_the_reads_and_bad_input_is_refused() {
    let off = world(false, arc(|_, _| Turn::Silent)).await;
    assert!(matches!(
        off.core.crew_flow_map("crew-1").await,
        Err(crate::error::CoreError::NotFound(_))
    ));
    assert!(matches!(
        off.core.crew_flow_messages("crew-1", None, page()),
        Err(crate::error::CoreError::NotFound(_))
    ));
    assert!(matches!(
        off.core.crew_flow_facts("crew-1"),
        Err(crate::error::CoreError::NotFound(_))
    ));

    let d = duo().await;
    assert!(d.w.core.crew_flow_map("../x").await.is_err());
    assert!(d.w.core.crew_flow_messages("a b", None, page()).is_err());
    assert!(d.w.core.crew_flow_facts("").is_err());
    let empty = d.w.core.crew_flow_map("nobody").await.unwrap();
    assert!(empty.nodes.is_empty() && empty.edges.is_empty() && !empty.truncated);
    assert!(
        d.w.core
            .crew_flow_messages("nobody", None, page())
            .unwrap()
            .messages
            .is_empty()
    );
    assert!(d.w.core.crew_flow_facts("nobody").unwrap().facts.is_empty());
}
