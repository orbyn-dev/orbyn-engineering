//! Thread lifecycle through the real service.

mod support;

use std::collections::BTreeSet;
use std::time::Duration;

use serde_json::{Value, json};
use support::{Options, Service, TempDir, setup};

/// A snooze time safely in the future.
const LATER: &str = "2999-01-01T00:00:00Z";

async fn list(service: &Service, project: &str, archived: bool) -> Vec<String> {
    service
        .json(&format!(
            "/api/threads?project_id={project}&archived={archived}"
        ))
        .await
        .as_array()
        .unwrap()
        .iter()
        .map(|thread| thread["id"].as_str().unwrap().to_owned())
        .collect()
}

async fn listed(service: &Service, project: &str, archived: bool) -> Vec<String> {
    list(service, project, archived).await
}

async fn project_of(service: &Service) -> String {
    service.json("/api/projects").await[0]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn new_threads_open_on_top_and_order_survives_activity() {
    let dir = TempDir::new();
    let (service, first) = setup(&dir, Options::default(), "codex").await;
    let project = project_of(&service).await;
    let second = service.thread(&project, "codex").await;
    let third = service.thread(&project, "codex").await;
    assert_eq!(
        listed(&service, &project, false).await,
        [third.as_str(), second.as_str(), first.as_str()]
    );
    service.admit(&first, "hi").await;
    service.finished(&first).await;
    assert_eq!(
        listed(&service, &project, false).await,
        [third.as_str(), second.as_str(), first.as_str()]
    );

    let (status, _) = service
        .send(
            "/api/threads/order",
            json!({ "ids": [first, third, second] }),
        )
        .await;
    assert_eq!(status, 204);
    assert_eq!(
        listed(&service, &project, false).await,
        [first.as_str(), third.as_str(), second.as_str()]
    );

    // A stale id rejects the whole list.
    let (status, _) = service
        .send(
            "/api/threads/order",
            json!({ "ids": [second, "missing", third] }),
        )
        .await;
    assert_eq!(status, 404);
    assert_eq!(
        listed(&service, &project, false).await,
        [first.as_str(), third.as_str(), second.as_str()]
    );

    let (status, pinned) = service.patch(&second, json!({ "pinned": true })).await;
    assert_eq!(status, 200);
    assert!(pinned["lifecycle"]["pinned_at"].is_string());
    assert_eq!(
        listed(&service, &project, false).await,
        [second.as_str(), first.as_str(), third.as_str()]
    );
}

#[tokio::test]
async fn archive_hides_blocks_runs_and_restores() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    let project = project_of(&service).await;
    assert_eq!(
        service.patch(&thread, json!({ "archived": true })).await.0,
        200
    );
    assert!(list(&service, &project, false).await.is_empty());
    assert_eq!(listed(&service, &project, true).await, [thread.as_str()]);
    let (status, body) = service
        .send(
            &format!("/api/threads/{thread}/runs"),
            json!({ "prompt": "x" }),
        )
        .await;
    assert_eq!(status, 409);
    assert_eq!(body["code"], "thread_archived");
    assert_eq!(
        service.patch(&thread, json!({ "archived": false })).await.0,
        200
    );
    service.admit(&thread, "works").await;
    service.finished(&thread).await;
}

#[tokio::test]
async fn busy_threads_refuse_settle_and_archive_and_the_patch_is_all_or_nothing() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    let run = service.admit(&thread, "[hang]").await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let (status, body) = service
        .patch(
            &thread,
            json!({ "pinned": true, "title": "T", "settled": true }),
        )
        .await;
    assert_eq!(status, 409);
    assert_eq!(body["code"], "thread_busy");
    let current = service.json(&format!("/api/threads/{thread}")).await;
    assert!(current["lifecycle"]["pinned_at"].is_null());
    assert_ne!(current["title"], "T");
    assert_eq!(
        service.patch(&thread, json!({ "archived": true })).await.0,
        409
    );
    service
        .send(&format!("/api/runs/{run}/cancel"), json!({}))
        .await;
    service.finished(&thread).await;
}

#[tokio::test]
async fn settle_unpins_and_a_new_message_restores_the_thread() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    service.patch(&thread, json!({ "pinned": true })).await;
    let (_, settled) = service.patch(&thread, json!({ "settled": true })).await;
    assert!(settled["lifecycle"]["settled_at"].is_string());
    assert!(settled["lifecycle"]["pinned_at"].is_null());
    service.admit(&thread, "back to work").await;
    let current = service.json(&format!("/api/threads/{thread}")).await;
    assert!(current["lifecycle"]["settled_at"].is_null());
    service.finished(&thread).await;
}

#[tokio::test]
async fn snooze_needs_a_future_time_and_wake_rules_end_it() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    for bad in ["2001-01-01T00:00:00Z", "tomorrow"] {
        let (status, _) = service
            .patch(&thread, json!({ "snoozed_until": bad }))
            .await;
        assert_eq!(status, 400, "{bad}");
    }
    let snoozed_until = |value: &Value| value["lifecycle"]["snoozed_until"].clone();
    let (_, body) = service
        .patch(&thread, json!({ "snoozed_until": LATER }))
        .await;
    assert!(snoozed_until(&body).is_string());
    let (_, body) = service.patch(&thread, json!({ "wake": true })).await;
    assert!(snoozed_until(&body).is_null());

    // A question for you ends the snooze while the run is still going.
    let run = service.admit(&thread, "[approve]").await;
    assert!(!run.is_empty());
    service
        .patch(&thread, json!({ "snoozed_until": LATER }))
        .await;
    let approval = service.pending().await;
    let body = service.json(&format!("/api/threads/{thread}")).await;
    assert!(snoozed_until(&body).is_null(), "{body}");

    // So does the answer.
    service
        .patch(&thread, json!({ "snoozed_until": LATER }))
        .await;
    service.decide(approval["id"].as_str().unwrap(), true).await;
    service.finished(&thread).await;
    let body = service.json(&format!("/api/threads/{thread}")).await;
    assert!(snoozed_until(&body).is_null(), "{body}");
}

#[tokio::test]
async fn delete_takes_descendants_but_refuses_a_busy_family() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    assert_eq!(
        service.patch(&thread, json!({ "unread": true })).await.1["lifecycle"]["unread"],
        true
    );
    let client = reqwest::Client::new();
    let response = client
        .delete(format!("{}/api/threads/{thread}", service.url))
        .bearer_auth(&service.token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);
    let audit = service.json("/api/audit?limit=5").await;
    assert_eq!(audit[0]["action"], "thread.delete");
}

/// Forks a child from a thread and checks the stored edge.
async fn fork(service: &Service, parent: &str, side: bool) -> String {
    let (status, thread) = service
        .send(
            &format!("/api/threads/{parent}/fork"),
            json!({ "side": side }),
        )
        .await;
    assert_eq!(status, 201, "{thread}");
    assert_eq!(thread["lifecycle"]["parent_thread_id"], parent);
    thread["id"].as_str().unwrap().to_owned()
}

async fn family(service: &Service, thread: &str) -> Value {
    service.json(&format!("/api/threads/{thread}/family")).await
}

/// The family read walks the stored `parent_thread_id` tree, so a fork, a side chat
/// and their own forks come back together, and a thread outside the tree never does.
#[tokio::test]
async fn a_thread_family_is_the_whole_parent_child_tree_and_leaves_unrelated_threads_out() {
    let dir = TempDir::new();
    let (service, root) = setup(&dir, Options::default(), "codex").await;
    let project = project_of(&service).await;
    let side = fork(&service, &root, true).await;
    let child = fork(&service, &side, false).await;
    let grandchild = fork(&service, &child, false).await;
    let peer = service.thread(&project, "codex").await;
    assert_eq!(
        service
            .patch(&grandchild, json!({ "archived": true }))
            .await
            .0,
        200
    );

    let expected = BTreeSet::from([
        root.as_str(),
        side.as_str(),
        child.as_str(),
        grandchild.as_str(),
    ]);
    // Asking any member gives the same tree, from the top.
    for asked in [&root, &side, &child, &grandchild] {
        let answer = family(&service, asked).await;
        assert_eq!(answer["thread_id"], asked.as_str(), "{answer}");
        assert_eq!(answer["root_thread_id"], root.as_str(), "{answer}");
        let members = answer["threads"].as_array().unwrap();
        let ids: Vec<&str> = members
            .iter()
            .map(|thread| thread["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            ids.iter().copied().collect::<BTreeSet<_>>(),
            expected,
            "{answer}"
        );
        assert_eq!(ids[0], root.as_str(), "the root comes first: {answer}");
        assert!(!ids.contains(&peer.as_str()), "{answer}");
        let position = |id: &str| ids.iter().position(|one| *one == id).unwrap();
        assert!(position(&root) < position(&side));
        assert!(position(&side) < position(&child));
        assert!(position(&child) < position(&grandchild));
        // The relation is the store's, not a second lineage.
        let member = |id: &str| {
            members
                .iter()
                .find(|thread| thread["id"] == id)
                .unwrap_or_else(|| panic!("{id} is in the family"))
                .clone()
        };
        assert!(member(&root)["lifecycle"]["parent_thread_id"].is_null());
        assert_eq!(member(&side)["lifecycle"]["relation"], "side");
        assert_eq!(member(&child)["lifecycle"]["relation"], "fork");
        assert_eq!(member(&grandchild)["lifecycle"]["relation"], "fork");
        // Archiving hides a thread from the list, not from its family.
        assert!(member(&grandchild)["lifecycle"]["archived_at"].is_string());
    }

    // An unrelated thread's family is itself alone.
    let alone = family(&service, &peer).await;
    assert_eq!(alone["root_thread_id"], peer.as_str());
    assert_eq!(alone["threads"].as_array().unwrap().len(), 1);

    let missing = service
        .get("/api/threads/not-a-thread/family")
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), 404);
}

#[tokio::test]
async fn rename_validates_and_audits() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    assert_eq!(
        service.patch(&thread, json!({ "title": "   " })).await.0,
        400
    );
    let (status, body) = service
        .patch(&thread, json!({ "title": "  Named  " }))
        .await;
    assert_eq!(status, 200);
    assert_eq!(body["title"], "Named");
    assert!(
        service
            .audit(&thread)
            .await
            .iter()
            .any(|row| row["action"] == "thread.rename")
    );
}

fn title_options(dir: &TempDir) -> (std::path::PathBuf, Options) {
    let record = dir.path().join("one-shot");
    let options = Options {
        env: vec![(
            "ORBYN_FAKE_ONE_SHOT_FILE".into(),
            record.to_string_lossy().into_owned(),
        )],
        ..Options::default()
    };
    (record, options)
}

async fn set_preference(service: &Service, values: Value) {
    let response = service
        .client
        .put(format!("{}/api/preferences", service.url))
        .bearer_auth(&service.token)
        .json(&json!({ "values": values }))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
}

fn args_file(record: &std::path::Path) -> std::path::PathBuf {
    record.with_extension("args")
}

async fn wait_for_title(service: &Service, thread: &str, title: &str) {
    for _ in 0..200 {
        if service.json(&format!("/api/threads/{thread}")).await["title"] == title {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the title never became {title}");
}

#[tokio::test]
async fn a_first_message_names_the_thread_once_and_only_then() {
    let dir = TempDir::new();
    let (record, options) = title_options(&dir);
    let (service, thread) = setup(&dir, options, "codex").await;
    let (status, _) = service
        .send(&format!("/api/threads/{thread}/title"), json!({}))
        .await;
    assert_eq!(status, 400, "an empty thread has nothing to title");
    service.admit(&thread, "please fix the login bug").await;
    service.finished(&thread).await;
    wait_for_title(&service, &thread, "A fresh title").await;
    let prompt = std::fs::read_to_string(&record).unwrap();
    assert!(prompt.contains("please fix the login bug"));
    let args = std::fs::read_to_string(args_file(&record)).unwrap();
    assert!(args.contains("fake-small"), "the small model: {args}");
    let calls = |rows: Vec<Value>| {
        rows.iter()
            .filter(|row| row["action"] == "engine.one_shot")
            .count()
    };
    assert_eq!(calls(service.audit(&thread).await), 1);

    std::fs::remove_file(&record).unwrap();
    service.admit(&thread, "and a second message").await;
    service.finished(&thread).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!record.exists(), "only the first message spends usage");
    assert_eq!(calls(service.audit(&thread).await), 1);
}

#[tokio::test]
async fn the_titles_setting_turns_the_first_message_call_off() {
    let dir = TempDir::new();
    let (record, options) = title_options(&dir);
    let (service, thread) = setup(&dir, options, "codex").await;
    set_preference(&service, json!({ "auto_thread_titles": false })).await;
    service.admit(&thread, "please fix the login bug").await;
    service.finished(&thread).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!record.exists(), "the setting stops the call");

    let (status, body) = service
        .send(&format!("/api/threads/{thread}/title"), json!({}))
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["title"], "A fresh title");
    let actions = service.audit(&thread).await;
    assert!(actions.iter().any(|row| row["action"] == "engine.one_shot"));
}

#[tokio::test]
async fn the_chosen_text_model_is_used_where_the_engine_lists_it() {
    let dir = TempDir::new();
    let (record, options) = title_options(&dir);
    let (service, thread) = setup(&dir, options, "codex").await;
    set_preference(&service, json!({ "text_generation_model": "fake-big" })).await;
    service.admit(&thread, "first").await;
    service.finished(&thread).await;
    wait_for_title(&service, &thread, "A fresh title").await;
    let args = std::fs::read_to_string(args_file(&record)).unwrap();
    assert!(args.contains("fake-big"), "{args}");
}

#[tokio::test]
async fn the_per_engine_text_model_names_the_first_message_title() {
    let dir = TempDir::new();
    let (record, options) = title_options(&dir);
    let (service, thread) = setup(&dir, options, "codex").await;
    set_preference(
        &service,
        json!({ "text_generation_models": { "codex": "fake-big", "claude": "fable" } }),
    )
    .await;
    service.admit(&thread, "first").await;
    service.finished(&thread).await;
    wait_for_title(&service, &thread, "A fresh title").await;
    let args = std::fs::read_to_string(args_file(&record)).unwrap();
    assert!(args.contains("fake-big"), "{args}");
    assert!(!args.contains("fable"), "{args}");
}

#[tokio::test]
async fn another_engines_entry_is_not_sent_to_this_titles_engine() {
    let dir = TempDir::new();
    let (record, options) = title_options(&dir);
    let (service, thread) = setup(&dir, options, "codex").await;
    set_preference(
        &service,
        json!({ "text_generation_models": { "claude": "fake-big" } }),
    )
    .await;
    service.admit(&thread, "first").await;
    service.finished(&thread).await;
    wait_for_title(&service, &thread, "A fresh title").await;
    let args = std::fs::read_to_string(args_file(&record)).unwrap();
    assert!(args.contains("fake-small"), "the small model: {args}");
    assert!(!args.contains("fake-big"), "{args}");
}

#[tokio::test]
async fn the_text_generators_read_lists_models_choices_and_project_scope() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    let project = service.json(&format!("/api/threads/{thread}")).await["project_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let engine = |read: &Value, id: &str| {
        read["engines"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["engine"] == id)
            .unwrap_or_else(|| panic!("{id} not in {read}"))
            .clone()
    };

    // The first read lists the catalog and resolves to the small model.
    let read = service.json("/api/text-generators").await;
    let codex = engine(&read, "codex");
    assert!(
        codex["models"]
            .as_array()
            .unwrap()
            .iter()
            .any(|model| model["id"] == "fake-big"),
        "{codex}"
    );
    assert!(codex["chosen"].is_null(), "{codex}");
    assert_eq!(codex["resolved"], "fake-small");
    assert_eq!(codex["reason"], "small");

    // A listed model is accepted, and the project read carries the same choice.
    set_preference(
        &service,
        json!({ "text_generation_models": { "codex": "fake-big" } }),
    )
    .await;
    let read = service
        .json(&format!("/api/text-generators?project={project}"))
        .await;
    let codex = engine(&read, "codex");
    assert_eq!(codex["chosen"], "fake-big");
    assert_eq!(codex["resolved"], "fake-big");
    assert_eq!(codex["reason"], "chosen");
    assert_eq!(codex["project"]["project_id"], project);
    assert_eq!(codex["project"]["chosen"], "fake-big");
    assert_eq!(codex["project"]["resolved"], "fake-big");
    assert_eq!(codex["project"]["reason"], "chosen");

    // Once the engine has listed, a model it does not offer is refused.
    let response = service
        .client
        .put(format!("{}/api/preferences", service.url))
        .bearer_auth(&service.token)
        .json(&json!({"values": {"text_generation_models": {"codex": "not-a-model"}}}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 400);
}

#[tokio::test]
async fn rename_during_title_generation_wins() {
    let dir = TempDir::new();
    let (_record, options) = title_options(&dir);
    let (service, thread) = setup(&dir, options, "claude").await;
    service.admit(&thread, "[slow-title]").await;
    service.finished(&thread).await;
    let path = format!("/api/threads/{thread}/title");
    let (generated, renamed) = tokio::join!(service.send(&path, json!({})), async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        service.patch(&thread, json!({ "title": "Mine" })).await
    });
    assert_eq!(renamed.0, 200);
    assert_eq!(generated.1["title"], "Mine");
}

#[tokio::test]
async fn failed_title_call_keeps_the_title_and_says_why() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "claude").await;
    service.patch(&thread, json!({ "title": "Kept" })).await;
    service.admit(&thread, "[bad-title]").await;
    service.finished(&thread).await;
    let (status, body) = service
        .send(&format!("/api/threads/{thread}/title"), json!({}))
        .await;
    assert_eq!(status, 502);
    assert!(body["message"].as_str().unwrap().contains("title failure"));
    assert_eq!(
        service.json(&format!("/api/threads/{thread}")).await["title"],
        "Kept"
    );
}

#[tokio::test]
async fn settings_round_trip_and_validate() {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), Options::default());
    let mut settings = service.json("/api/settings").await;
    assert_eq!(settings["auto_settle"]["after_days"], 3);
    settings["title_instructions"] = json!("Use sentence case");
    let response = service
        .client
        .put(format!("{}/api/settings", service.url))
        .bearer_auth(&service.token)
        .json(&settings)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        service.json("/api/settings").await["title_instructions"],
        "Use sentence case"
    );
    settings["snooze_presets"] = json!(vec![json!({ "label": "x", "minutes": 5 }); 9]);
    let response = service
        .client
        .put(format!("{}/api/settings", service.url))
        .bearer_auth(&service.token)
        .json(&settings)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
}

async fn search(service: &Service, query: &[(&str, &str)]) -> Value {
    let response = service
        .get("/api/search/messages")
        .query(query)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    response.json().await.unwrap()
}

#[tokio::test]
async fn message_search_opens_at_the_message_and_keeps_to_its_scope() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    service.admit(&thread, "rename the billing helper").await;
    service.finished(&thread).await;

    let found = search(&service, &[("q", "HELPER billing")]).await;
    assert_eq!(found["too_short"], false);
    let hit = &found["hits"][0];
    assert_eq!(hit["thread_id"], thread.as_str());
    assert_eq!(hit["role"], "user");
    assert_eq!(hit["snippet"], "rename the billing helper");
    let events = service.all_events(&thread).await;
    let at = events
        .iter()
        .find(|event| event["seq"] == hit["seq"])
        .expect("the hit points into the log");
    assert_eq!(at["event"]["messageId"], hit["message_id"]);

    let answer = search(&service, &[("q", "hello codex")]).await;
    assert_eq!(answer["hits"][0]["role"], "assistant");
    let split = search(&service, &[("q", "billing hello")]).await;
    assert_eq!(split["hits"], json!([]), "the words are in two messages");

    let project = service.json(&format!("/api/threads/{thread}")).await["project_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let inside = search(&service, &[("q", "billing"), ("project_id", &project)]).await;
    assert_eq!(inside["hits"].as_array().unwrap().len(), 1);
    let elsewhere = search(&service, &[("q", "billing"), ("project_id", "other")]).await;
    assert_eq!(elsewhere["hits"], json!([]));

    let short = search(&service, &[("q", "a")]).await;
    assert_eq!(short["too_short"], true);
    let long = "x".repeat(201);
    let refused = service
        .get("/api/search/messages")
        .query(&[("q", long.as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), 400);

    let (code, _) = service.patch(&thread, json!({ "archived": true })).await;
    assert_eq!(code, 200);
    assert_eq!(
        search(&service, &[("q", "billing")]).await["hits"],
        json!([])
    );
    let archived = search(&service, &[("q", "billing"), ("archived", "true")]).await;
    assert_eq!(archived["hits"].as_array().unwrap().len(), 1);

    let response = service
        .client
        .delete(format!("{}/api/threads/{thread}", service.url))
        .bearer_auth(&service.token)
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    let gone = search(&service, &[("q", "billing"), ("archived", "true")]).await;
    assert_eq!(
        gone["hits"],
        json!([]),
        "deleting a thread deletes its text"
    );
}

#[tokio::test]
async fn message_search_streams_the_same_matches_in_order() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    service.admit(&thread, "rename the billing helper").await;
    service.finished(&thread).await;
    service
        .admit(&thread, "billing again after the rename")
        .await;
    service.finished(&thread).await;

    let whole = search(&service, &[("q", "billing")]).await;
    assert_eq!(
        whole["hits"].as_array().unwrap().len(),
        2,
        "two prompts mention billing"
    );
    let frames = service
        .sse_until("/api/search/messages/events?q=billing", |frame| {
            frame["type"] == "done"
        })
        .await;
    let streamed: Vec<&Value> = frames
        .iter()
        .filter(|frame| frame["type"] == "hit")
        .map(|frame| &frame["hit"])
        .collect();
    assert_eq!(
        streamed,
        whole["hits"].as_array().unwrap().iter().collect::<Vec<_>>(),
        "the stream returns the read's hits in the read's order"
    );
    let done = frames.last().unwrap();
    assert_eq!(done["type"], "done");
    assert_eq!(done["too_short"], false);
    assert_eq!(done["truncated"], whole["truncated"]);

    // A limit ends the stream early, exactly as it truncates the whole read.
    let limited = service
        .sse_until("/api/search/messages/events?q=billing&limit=1", |frame| {
            frame["type"] == "done"
        })
        .await;
    assert_eq!(
        limited
            .iter()
            .filter(|frame| frame["type"] == "hit")
            .count(),
        1
    );
    assert_eq!(limited.last().unwrap()["truncated"], true);

    // No matches still finishes with a frame that says so.
    let none = service
        .sse_until("/api/search/messages/events?q=billing%20zebra", |frame| {
            frame["type"] == "done"
        })
        .await;
    assert_eq!(none.len(), 1);
    assert_eq!(none[0]["type"], "done");
    assert_eq!(none[0]["truncated"], false);

    // A too-short query answers as the read does, without searching.
    let short = service
        .sse_until("/api/search/messages/events?q=a", |frame| {
            frame["type"] == "done"
        })
        .await;
    assert_eq!(short.len(), 1);
    assert_eq!(short[0]["too_short"], true);

    // A query the read refuses is refused before any stream starts.
    let refused = service
        .get("/api/search/messages/events")
        .query(&[("q", "x".repeat(201))])
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), 400);
    service.stop().await;
}

#[tokio::test]
async fn a_fork_is_searchable_and_a_rewind_hides_what_it_undid() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    let run = service.admit(&thread, "first walrus prompt").await;
    service.finished(&thread).await;
    let (status, fork) = service
        .send(&format!("/api/threads/{thread}/fork"), json!({}))
        .await;
    assert_eq!(status, 201, "{fork}");
    let found = search(&service, &[("q", "walrus")]).await;
    assert_eq!(found["hits"].as_array().unwrap().len(), 2);

    let (status, body) = service
        .send(
            &format!("/api/threads/{thread}/rewind"),
            json!({ "run_id": run, "restore_files": false }),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let found = search(&service, &[("q", "walrus")]).await;
    assert_eq!(found["hits"].as_array().unwrap().len(), 1);
    assert_eq!(found["hits"][0]["thread_id"], fork["id"]);
}
