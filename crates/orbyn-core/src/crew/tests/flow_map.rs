//! The Map, message and fact reads on two fake engines (`alpha` is steered mid-turn, `beta` is not): the nodes, edges and
//! delivery states are right, every type has exactly the fields it should, the reads change nothing, the flag off hides them, and
//! nothing a member wrote (a message body, a fact value) reaches any read of the crew. No test depends on the clock.

use orbyn_contracts::{CrewObject, FlowDeliveryMode, FlowRole, object_id};
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

#[tokio::test]
async fn a_delivery_is_reported_only_when_every_recipient_agrees() {
    let marker = nonce("agree");
    let (d, crew) = talked(&marker).await;
    let read = |d: &Duo| {
        d.w.core
            .crew_flow_messages(&crew, None, page())
            .unwrap()
            .messages
    };
    let messages = read(&d);
    let (held, live) = (messages[0].id.clone(), messages[1].id.clone());
    // One recipient each: its own state.
    assert_eq!(messages[0].delivery, Some(FlowDeliveryMode::Held));
    assert_eq!(messages[1].delivery, Some(FlowDeliveryMode::Live));

    // The newest row of a recipient is the one that counts: the held message goes out with the recipient's next tool result.
    let store = &d.w.core.store;
    store
        .record_crew_delivery(&held, &d.mb, "thread-b", "tool_result", None)
        .unwrap();
    assert_eq!(read(&d)[0].delivery, Some(FlowDeliveryMode::ToolResult));

    // Every recipient delivered the same way: that way.
    store
        .record_crew_delivery(&live, "member-extra", "thread-extra", "live", None)
        .unwrap();
    assert_eq!(read(&d)[1].delivery, Some(FlowDeliveryMode::Live));

    // One recipient live and one failed: no delivery is reported, not either one's.
    store
        .record_crew_delivery(&held, "member-other", "thread-other", "failed", None)
        .unwrap();
    store
        .record_crew_delivery(&live, "member-late", "thread-late", "failed", None)
        .unwrap();
    let mixed = read(&d);
    assert_eq!(
        (mixed[0].delivery, mixed[0].delivered_at.clone()),
        (None, None)
    );
    assert_eq!(mixed[1].delivery, None);
    let map = d.w.core.crew_flow_map(&crew).await.unwrap();
    assert!(
        map.edges.iter().all(|e| e.delivery.is_none()),
        "an edge says nothing its newest message does not"
    );

    // A message with no delivery row has none.
    let mut message =
        CrewObject::Message(d.w.core.store.crew_messages(&d.a).unwrap().remove(0)).to_value();
    message["body"] = json!(format!("unsent {marker}"));
    message["id"] = json!(object_id(&message).unwrap());
    d.w.core
        .store
        .add_crew_member_object(&d.a, &CrewObject::parse(&message).unwrap())
        .unwrap();
    let newest = read(&d).remove(0);
    assert_eq!((newest.delivery, newest.delivered_at), (None, None));
}

#[tokio::test]
async fn the_last_event_lookup_seeks_the_primary_key_and_never_scans() {
    let d = duo().await;
    let sql = format!("EXPLAIN QUERY PLAN {}", crate::store::last_events_sql(3));
    let conn = d.w.core.store.reader();
    let mut statement = conn.prepare(&sql).unwrap();
    let plan: Vec<String> = statement
        .query_map(rusqlite::params_from_iter(["a", "b", "c"]), |row| {
            row.get::<_, String>(3)
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        plan.iter()
            .any(|p| p.contains("thread_events") && p.contains("SEARCH")),
        "the plan has no seek"
    );
    assert!(
        !plan.iter().any(|p| p.contains("SCAN thread_events")),
        "the plan scans the events"
    );
}

#[tokio::test]
async fn a_nodes_outcome_and_lease_are_scrubbed_of_the_crews_key_text() {
    // A task stored with its brief and key, as a lead that slipped would leave it: the outcome and a lease path hold a check's text.
    let w = world(true, arc(|_, _| Turn::Silent)).await;
    let secret = nonce("secret");
    let stamper = crate::crew::Stamper {
        crew_id: orbyn_contracts::CrewId::new("crew-1").unwrap(),
        engine: orbyn_contracts::CrewToken::new("claude").unwrap(),
        member: orbyn_contracts::CrewMember::new("lead-01").unwrap(),
        now: orbyn_contracts::CrewTimestamp::new("2026-11-12T10:00:00Z").unwrap(),
    };
    let brief = stamper
        .brief(&json!({
            "kind": "implement",
            "outcome": format!("Run {secret} and report."),
            "inputs": [],
            "files": [{ "path": format!("src/{secret}/**"), "mode": "write" }],
            "allowed_effects": ["read_repo", "write_leased", "run_checks"],
            "stop_when": ["Done."],
            "budget": { "turns": 20, "minutes": 30 }
        }))
        .unwrap();
    let key = stamper
        .answer_key(
            &json!({
                "checks": [{ "id": "chk-run", "kind": "command", "params": { "run": secret }, "proves": "It runs." }],
                "protected_paths": ["tests/**"]
            }),
            &brief.id,
        )
        .unwrap();
    let task = w
        .core
        .store
        .create_crew_task(&brief, &key, "run-scrub")
        .unwrap();
    w.core
        .store
        .add_crew_run(&task.id, "lead-thread", "worker-01")
        .unwrap();

    let map = w.core.crew_flow_map("crew-1").await.unwrap();
    let node = map
        .nodes
        .iter()
        .find(|n| n.task.as_deref() == Some(task.id.as_str()))
        .unwrap();
    assert!(
        !node.outcome.is_empty() && !node.lease.is_empty(),
        "the node has an outcome and a lease"
    );
    let text = serde_json::to_string(&map).unwrap();
    assert!(
        !text.contains(&secret),
        "a check's text reached the map through an outcome or a lease"
    );
}

/// A task of a second crew, live and with a worker, and `count` messages stored under it, each with a delivery row: traffic of its own.
fn other_crews_traffic(d: &Duo, count: usize) {
    let stamper = crate::crew::Stamper {
        crew_id: orbyn_contracts::CrewId::new("crew-2").unwrap(),
        engine: orbyn_contracts::CrewToken::new("claude").unwrap(),
        member: orbyn_contracts::CrewMember::new("lead-02").unwrap(),
        now: orbyn_contracts::CrewTimestamp::new("2026-11-12T10:00:00Z").unwrap(),
    };
    let brief = stamper
        .brief(&json!({
            "kind": "implement", "outcome": "Another crew's task.", "inputs": [],
            "files": [{ "path": "other/**", "mode": "write" }],
            "allowed_effects": ["read_repo", "write_leased", "run_checks"],
            "stop_when": ["Done."], "budget": { "turns": 20, "minutes": 30 }
        }))
        .unwrap();
    let key = stamper
        .answer_key(
            &json!({ "checks": [{ "id": "chk-run", "kind": "command", "params": { "run": "true" }, "proves": "It runs." }],
                     "protected_paths": ["tests/**"] }),
            &brief.id,
        )
        .unwrap();
    let store = &d.w.core.store;
    let task = store.create_crew_task(&brief, &key, "run-other").unwrap();
    store
        .add_crew_run(&task.id, "lead-thread-2", "worker-other")
        .unwrap();
    let crew = d.w.task(&d.a).crew_id;
    let (_, template) = store.crew_messages_in_crew(&crew).unwrap().remove(0);
    for n in 0..count {
        let mut value = CrewObject::Message(template.clone()).to_value();
        value["crew_id"] = json!("crew-2");
        value["run_id"] = json!(task.run_id);
        value["body"] = json!(format!("other {n}"));
        value["id"] = json!(object_id(&value).unwrap());
        let object = CrewObject::parse(&value).unwrap();
        store.add_crew_member_object(&task.id, &object).unwrap();
        store
            .record_crew_delivery(
                object.id().as_str(),
                "worker-other",
                "thread-other",
                "held",
                None,
            )
            .unwrap();
    }
}

#[tokio::test]
async fn another_crews_traffic_never_cuts_a_crews_window_and_a_cut_is_reported() {
    let marker = nonce("window");
    let (d, crew) = talked(&marker).await;
    other_crews_traffic(&d, 3);
    let store = &d.w.core.store;
    // The crew has two messages and two deliveries. A window of exactly that holds them all, whatever another crew wrote since.
    let (edges, cut) = store.crew_message_edges(&crew, 200, 2).unwrap();
    assert_eq!((edges.len(), cut), (2, false));
    assert!(edges.iter().all(|e| e.count == 1));
    let (held, cut) = store.crew_held_counts(&crew, 2).unwrap();
    assert_eq!((held, cut), (vec![(d.mb.clone(), 1)], false));
    // The other crew's own counts are its own.
    let (other, cut) = store.crew_message_edges("crew-2", 200, 5000).unwrap();
    assert_eq!(
        (other.iter().map(|e| e.count).sum::<u32>(), cut),
        (3, false)
    );

    // A window of one cuts the crew's older message, and the read says so: the newest message's edge is the one left.
    let (edges, cut) = store.crew_message_edges(&crew, 200, 1).unwrap();
    assert_eq!((edges.len(), cut), (1, true));
    assert_eq!(edges[0].sender, d.ma);
    let (_, cut) = store.crew_held_counts(&crew, 1).unwrap();
    assert!(cut, "the delivery window cut the crew's older rows");
    // The map reads with the real windows, which these few rows do not reach.
    assert!(!d.w.core.crew_flow_map(&crew).await.unwrap().truncated);
}

#[tokio::test]
async fn a_fan_out_that_has_not_reached_every_recipient_reports_no_delivery() {
    let marker = nonce("fanout");
    let (d, crew) = talked(&marker).await;
    // The lead writes to everyone: both workers were running, so two recipients.
    let (_, error) =
        d.w.lead(
            "crew_post",
            json!({ "to": "all", "kind": "fact", "body": format!("all {marker}") }),
        )
        .await;
    assert!(!error, "the broadcast was refused");
    let store = &d.w.core.store;
    let (task, lead_message) = store
        .crew_messages_in_crew(&crew)
        .unwrap()
        .into_iter()
        .find(|(_, m)| m.to.as_str() == "all")
        .unwrap();
    // The same message again with no delivery row yet, as a fan-out would be part-way through its loop.
    let mut value = CrewObject::Message(lead_message).to_value();
    value["body"] = json!(format!("again {marker}"));
    value["id"] = json!(object_id(&value).unwrap());
    let object = CrewObject::parse(&value).unwrap();
    store.add_crew_member_object(&task, &object).unwrap();
    let id = object.id().as_str().to_owned();
    let newest = |d: &Duo| {
        d.w.core
            .crew_flow_messages(&crew, None, page())
            .unwrap()
            .messages
            .remove(0)
    };

    assert_eq!(newest(&d).id, id);
    assert_eq!(newest(&d).delivery, None, "no recipient has a row yet");
    store
        .record_crew_delivery(&id, &d.ma, "thread-a", "live", None)
        .unwrap();
    assert_eq!(
        newest(&d).delivery,
        None,
        "one of two recipients has a row, so nothing is reported"
    );
    assert_eq!(newest(&d).delivered_at, None);
    store
        .record_crew_delivery(&id, &d.mb, "thread-b", "live", None)
        .unwrap();
    assert_eq!(
        newest(&d).delivery,
        Some(FlowDeliveryMode::Live),
        "every recipient has the same row"
    );
}

/// A message to everyone with a delivery row for each of the two workers: delivered, as far as the rows go.
async fn delivered_broadcast(marker: &str) -> (Duo, String) {
    let (d, crew) = talked(marker).await;
    let (_, error) =
        d.w.lead(
            "crew_post",
            json!({ "to": "all", "kind": "fact", "body": format!("all {marker}") }),
        )
        .await;
    assert!(!error, "the broadcast was refused");
    let store = &d.w.core.store;
    let (task, lead_message) = store
        .crew_messages_in_crew(&crew)
        .unwrap()
        .into_iter()
        .find(|(_, m)| m.to.as_str() == "all")
        .unwrap();
    let mut value = CrewObject::Message(lead_message).to_value();
    value["body"] = json!(format!("again {marker}"));
    value["id"] = json!(object_id(&value).unwrap());
    let object = CrewObject::parse(&value).unwrap();
    store.add_crew_member_object(&task, &object).unwrap();
    let id = object.id().as_str().to_owned();
    store
        .record_crew_delivery(&id, &d.ma, "thread-a", "live", None)
        .unwrap();
    store
        .record_crew_delivery(&id, &d.mb, "thread-b", "live", None)
        .unwrap();
    (d, crew)
}

#[tokio::test]
async fn a_crew_with_more_tasks_than_were_read_reports_no_broadcast_delivery() {
    let (d, crew) = delivered_broadcast(&nonce("cap")).await;
    let newest = |cap: u32| {
        d.w.core
            .flow_messages_with(&crew, None, page(), cap)
            .unwrap()
            .messages
            .remove(0)
    };
    // Both workers were read: two recipients, two rows.
    assert_eq!(newest(200).delivery, Some(FlowDeliveryMode::Live));
    assert_eq!(
        newest(2).delivery,
        Some(FlowDeliveryMode::Live),
        "exactly as many tasks as the cap is not a cut"
    );
    // One task was left out: who else could have been sent to is unknown, so no delivery is reported whatever the rows say.
    assert_eq!(newest(1).delivery, None);
    assert_eq!(newest(1).delivered_at, None);
}

#[tokio::test]
async fn the_map_says_truncated_when_either_window_cuts_the_crews_traffic() {
    let (d, crew) = talked(&nonce("cuts")).await;
    let truncated = |messages: u32, deliveries: u32| {
        let core = d.w.core.clone();
        let crew = crew.clone();
        async move {
            core.flow_map_with(&crew, messages, deliveries)
                .await
                .unwrap()
                .truncated
        }
    };
    assert!(!truncated(5000, 20000).await, "nothing is cut");
    assert!(
        truncated(1, 20000).await,
        "the message window cut a message"
    );
    assert!(
        truncated(5000, 1).await,
        "the delivery window cut a delivery, and only that"
    );
}
