//! The Tools and connectors lists, through the real service and the stand-in engines.

mod support;

use serde_json::Value;
use support::{Options, TempDir, setup};

fn group<'a>(tools: &'a Value, id: &str) -> Option<&'a Value> {
    tools["groups"]
        .as_array()?
        .iter()
        .find(|one| one["id"] == id)
}

fn names(group: &Value) -> Vec<&str> {
    group["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn a_claude_thread_reports_servers_connectors_and_what_its_run_reported() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "claude").await;
    let path = format!("/api/threads/{thread}/tools");

    // Before the first run the CLI is asked, and what only a run reports is left out.
    let before = service.json(&path).await;
    assert_eq!(before["reported"], false);
    assert_eq!(
        names(group(&before, "connectors").unwrap()),
        ["Vercel", "Google Drive"]
    );
    for id in ["builtin", "orbyn", "plugins"] {
        assert!(
            before["not_reported"]
                .as_array()
                .unwrap()
                .contains(&id.into()),
            "{id}"
        );
    }

    service.admit(&thread, "hello").await;
    assert_eq!(service.finished(&thread).await, "completed");
    let after = service.json(&format!("{path}?refresh=1")).await;
    assert_eq!(after["reported"], true);
    assert_eq!(after["source"], "run");
    let mcp = group(&after, "mcp").unwrap();
    assert_eq!(names(mcp), ["github", "postgres-dev"]);
    assert_eq!(mcp["items"][1]["state"], "failed");
    assert_eq!(mcp["items"][1]["reason"], "Command not found: pg-mcp");
    let connectors = group(&after, "connectors").unwrap();
    assert_eq!(connectors["items"][1]["state"], "needs_sign_in");
    assert!(connectors["items"][1]["sign_in_url"].is_string());
    // The CLI's own plugins are not the person's.
    assert_eq!(names(group(&after, "plugins").unwrap()), ["orbyn-skills"]);
    assert_eq!(group(&after, "orbyn").unwrap()["count"], 2);
    assert_eq!(
        names(group(&after, "builtin").unwrap()),
        ["Bash", "Read", "Edit"]
    );
    assert_eq!(group(&after, "skills").unwrap()["count"], 3);

    // Settings reads the same engine without a thread, from the newest run's report.
    let settings = service.json("/api/engines/claude/tools").await;
    assert_eq!(settings["reported"], true);
    assert_eq!(
        names(group(&settings, "mcp").unwrap()),
        ["github", "postgres-dev"]
    );
    service.stop().await;
}

#[tokio::test]
async fn a_codex_thread_reports_only_what_codex_does_and_names_the_rest() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    let tools = service.json(&format!("/api/threads/{thread}/tools")).await;
    assert_eq!(tools["engine"], "codex");
    assert_eq!(
        names(group(&tools, "mcp").unwrap()),
        ["filesystem", "notion"]
    );
    assert_eq!(
        group(&tools, "mcp").unwrap()["items"][1]["state"],
        "needs_sign_in"
    );
    assert_eq!(group(&tools, "orbyn").unwrap()["count"], 2);
    // Only the apps the person can reach are theirs.
    assert_eq!(names(group(&tools, "connectors").unwrap()), ["GitHub"]);
    assert_eq!(names(group(&tools, "plugins").unwrap()), ["orbyn-skills"]);
    assert!(group(&tools, "builtin").is_none());
    assert!(
        tools["not_reported"]
            .as_array()
            .unwrap()
            .contains(&"builtin".into())
    );
    // The server's own address and tokens never ride along.
    let text = tools.to_string();
    assert!(!text.contains("http"), "{text}");
    service.stop().await;
}

#[tokio::test]
async fn an_engine_that_cannot_report_is_refused_plainly() {
    let dir = TempDir::new();
    let (service, _) = setup(&dir, Options::default(), "claude").await;
    let unknown = service
        .get("/api/engines/nonexistent/tools")
        .send()
        .await
        .unwrap();
    assert_eq!(unknown.status().as_u16(), 404);
    service.stop().await;
}

#[tokio::test]
async fn asking_again_reads_the_engine_again_and_the_answer_stays_whole() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    let path = format!("/api/threads/{thread}/tools");
    let first = service.json(&path).await;
    // A second ask inside the kept window and a forced one give the same list.
    let again = service.json(&path).await;
    let forced = service.json(&format!("{path}?refresh=1")).await;
    assert_eq!(group(&first, "connectors"), group(&again, "connectors"));
    assert_eq!(group(&first, "connectors"), group(&forced, "connectors"));
    assert!(forced.get("error").is_none(), "{forced}");
    service.stop().await;
}
