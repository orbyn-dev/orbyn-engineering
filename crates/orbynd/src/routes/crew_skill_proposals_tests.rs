//! Crew skill proposals over HTTP (KC9, docs/crew-lessons.md): the list in every state with its source lessons' evidence,
//! approve (which writes the local skill) and dismiss, their staleness, open-state, name and secret refusals, the person
//! key, and the Preview gate. Every write is a Person route that needs the person key, the read is Local, and with Preview
//! off every route answers as a route that is not there. The store and core rules are tested in `orbyn-core`
//! (`store/crew_skill_proposals_tests.rs`, `crew/tests/proposals.rs`); this file only observes them over HTTP. The table,
//! credential, remote and flag-off walks of `crew_tests.rs` cover these routes too, since they sit under `/api/crew`.

use orbyn_contracts::WorkspaceSetup;
use orbyn_core::crew::fixtures;
use orbyn_core::tools::local_skills::LocalSkillWrite;
use serde_json::{Value, json};

use super::crew_tasks_tests::{Served, serve, serve_with};
use crate::http::access::Access;

const PROPOSALS: &str = "/api/crew/skill-proposals";
const SECRET_REFUSAL: &str =
    "Not saved: this looks like it holds a secret. Edit it and save again.";
const SKILL_TOO_LONG: &str = "A skill is 1 to 600 characters.";

/// One project row for the proposal routes.
fn project(app: &Served, name: &str) -> String {
    fixtures::add_project(
        &app.core,
        app.core.config.data_dir.join(name).to_str().unwrap(),
    )
}

/// One request with the service token, and the person key when this test's `Access` asks for one.
async fn call(
    app: &Served,
    method: &str,
    path: &str,
    body: Option<Value>,
    key: Option<&str>,
) -> (u16, Value) {
    let mut request = reqwest::Client::new()
        .request(
            method.parse().unwrap(),
            format!("http://127.0.0.1:{}{path}", app.port),
        )
        .bearer_auth("token");
    if let Some(body) = body {
        request = request.json(&body);
    }
    if let Some(key) = key {
        request = request.header("x-orbyn-person", key);
    }
    let response = request.send().await.unwrap();
    let status = response.status().as_u16();
    let text = response.text().await.unwrap();
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

async fn get(app: &Served, path: &str) -> (u16, Value) {
    call(app, "GET", path, None, None).await
}

async fn write(app: &Served, method: &str, path: &str, body: Value) -> (u16, Value) {
    call(app, method, path, Some(body), None).await
}

/// The one open proposal's stored row, for the `rev` and template text a screen would show.
fn stored_proposal(app: &Served, id: &str) -> orbyn_core::store::CrewSkillProposal {
    app.core.store.skill_proposal(id).unwrap().unwrap()
}

fn source_lesson(app: &Served, id: &str) -> orbyn_core::store::CrewLesson {
    app.core.store.crew_lesson(id).unwrap().unwrap()
}

/// Whether the store's ledger holds this audit row.
fn audited(app: &Served, action: &str, target: &str) -> bool {
    app.core
        .store
        .list_audit(None, 500)
        .unwrap()
        .iter()
        .any(|row| row.action == action && row.target == target)
}

#[tokio::test]
async fn the_list_is_empty_until_a_proposal_exists_and_needs_a_repo() {
    let app = serve("proposals-empty", true, false).await;
    let repo = project(&app, "empty");
    let (status, empty) = get(&app, &format!("{PROPOSALS}?repo={repo}")).await;
    assert_eq!(status, 200);
    assert_eq!(empty, json!({ "proposals": [] }));

    let (status, body) = get(&app, PROPOSALS).await;
    assert_eq!((status, &body["code"]), (400, &json!("invalid")));
    let (status, body) = get(&app, &format!("{PROPOSALS}?repo=nobody")).await;
    assert_eq!((status, &body["code"]), (404, &json!("not_found")));
}

#[tokio::test]
async fn three_learned_lessons_make_one_open_proposal_with_their_evidence() {
    let app = serve("proposals-open", true, false).await;
    let repo = project(&app, "open");
    let proposal_id = fixtures::skill_proposal(&app.core, &repo, "src", &["esc-one", "task-two"]);
    let (status, list) = get(&app, &format!("{PROPOSALS}?repo={repo}")).await;
    assert_eq!(status, 200);
    let proposals = list["proposals"].as_array().unwrap();
    assert_eq!(proposals.len(), 1);
    let proposal = &proposals[0];
    assert_eq!(proposal["id"], proposal_id.as_str());
    assert_eq!(proposal["repo"], repo.as_str());
    assert_eq!(proposal["state"], "open");
    assert_eq!(proposal["can_approve"], true);
    assert_eq!(proposal["skill"], Value::Null);
    assert_eq!(proposal["approved_at"], Value::Null);
    assert_eq!(proposal["rev"], 1);
    let text = proposal["text"].as_str().unwrap();
    assert!(text.contains("- The third escape."), "{text}");
    let lessons = proposal["lessons"].as_array().unwrap();
    assert_eq!(lessons.len(), 3);
    for lesson in lessons {
        assert!(
            lesson["id"]
                .as_str()
                .is_some_and(|id| id.starts_with("les_"))
        );
        assert_eq!(
            lesson["evidence"],
            json!([
                { "kind": "escape", "id": "esc-one" },
                { "kind": "escape", "id": "task-two" }
            ])
        );
    }

    // A source lesson switched off keeps the proposal but makes it unapprovable.
    let lesson = source_lesson(&app, proposal["lessons"][0]["id"].as_str().unwrap());
    let (status, off) = write(
        &app,
        "POST",
        &format!("/api/crew/lessons/{}/disable", lesson.id),
        json!({ "rev": lesson.rev }),
    )
    .await;
    assert_eq!((status, off["state"].as_str()), (200, Some("disabled")));
    let (status, list) = get(&app, &format!("{PROPOSALS}?repo={repo}")).await;
    assert_eq!(status, 200);
    let proposal = &list["proposals"][0];
    assert_eq!(proposal["state"], "open");
    assert_eq!(proposal["rev"], 1);
    assert_eq!(proposal["can_approve"], false);
}

#[tokio::test]
async fn approving_writes_the_skill_and_the_second_approve_is_not_open() {
    let app = serve("proposals-approve", true, false).await;
    let repo = project(&app, "approve");
    let proposal_id = fixtures::skill_proposal(&app.core, &repo, "src", &["esc-one"]);
    let stored = stored_proposal(&app, &proposal_id);
    let path = format!("{PROPOSALS}/{proposal_id}/approve");
    let body = json!({ "text": stored.text, "rev": stored.rev });

    let (status, approved) = write(&app, "POST", &path, body.clone()).await;
    assert_eq!(status, 200, "{approved}");
    assert_eq!(approved["id"], proposal_id.as_str());
    assert_eq!(approved["state"], "approved");
    assert_eq!(approved["can_approve"], false);
    assert_eq!(approved["rev"], 2);
    assert_eq!(approved["skill"], json!({ "name": "crew-escape-src" }));
    assert_eq!(approved["text"], stored.text.as_str());
    assert!(
        approved["approved_at"]
            .as_str()
            .is_some_and(|at| at.ends_with('Z') && at.len() >= 20),
        "{approved}"
    );
    assert!(app.core.skills.local_skill_exists("crew-escape-src"));
    assert!(audited(&app, "crew.proposal.approve", &proposal_id));

    let (status, body) = write(&app, "POST", &path, body).await;
    assert_eq!((status, &body["code"]), (409, &json!("not_open")));

    // The read keeps the approved proposal, so the web sees it after the answer.
    let (status, list) = get(&app, &format!("{PROPOSALS}?repo={repo}")).await;
    assert_eq!(status, 200);
    let proposal = &list["proposals"][0];
    assert_eq!(proposal["state"], "approved");
    assert_eq!(proposal["skill"], json!({ "name": "crew-escape-src" }));
    assert_eq!(proposal["approved_at"], approved["approved_at"]);
    assert_eq!(proposal["can_approve"], false);
}

#[tokio::test]
async fn approve_refuses_a_stale_rev_bad_text_a_secret_and_unready_sources() {
    let app = serve("proposals-approve-refusals", true, false).await;
    let repo = project(&app, "approve-refusals");
    let proposal_id = fixtures::skill_proposal(&app.core, &repo, "src", &["esc-one"]);
    let stored = stored_proposal(&app, &proposal_id);
    let path = format!("{PROPOSALS}/{proposal_id}/approve");

    let (status, body) = write(
        &app,
        "POST",
        &path,
        json!({ "text": stored.text, "rev": stored.rev + 1 }),
    )
    .await;
    assert_eq!((status, &body["code"]), (409, &json!("stale")));
    assert!(!app.core.skills.local_skill_exists("crew-escape-src"));

    for (text, message) in [
        (String::new(), SKILL_TOO_LONG),
        ("x".repeat(601), SKILL_TOO_LONG),
    ] {
        let (status, body) = write(
            &app,
            "POST",
            &path,
            json!({ "text": text, "rev": stored.rev }),
        )
        .await;
        assert_eq!((status, &body["code"]), (400, &json!("invalid")));
        assert_eq!(body["message"], message);
    }
    let secret = format!("The token is ghp_{}.", "q".repeat(36));
    let (status, body) = write(
        &app,
        "POST",
        &path,
        json!({ "text": secret, "rev": stored.rev }),
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(body["code"], "secret_refused");
    assert_eq!(body["message"], SECRET_REFUSAL);
    assert!(!app.core.skills.local_skill_exists("crew-escape-src"));

    // A disabled source makes the proposal unapprovable even with the right revision.
    let lesson = source_lesson(&app, &stored.lessons[0]);
    write(
        &app,
        "POST",
        &format!("/api/crew/lessons/{}/disable", lesson.id),
        json!({ "rev": lesson.rev }),
    )
    .await;
    let (status, body) = write(
        &app,
        "POST",
        &path,
        json!({ "text": stored.text, "rev": stored.rev }),
    )
    .await;
    assert_eq!((status, &body["code"]), (400, &json!("invalid")));

    let (status, body) = write(
        &app,
        "POST",
        &format!("{PROPOSALS}/prop_missing/approve"),
        json!({ "text": stored.text, "rev": 1 }),
    )
    .await;
    assert_eq!((status, &body["code"]), (404, &json!("not_found")));
}

#[tokio::test]
async fn approve_refuses_a_name_the_library_or_a_local_skill_holds() {
    let app = serve("proposals-names", true, false).await;
    let repo = project(&app, "names");
    let library = app.core.config.data_dir.join("library");
    std::fs::create_dir_all(library.join("skills/how")).unwrap();
    std::fs::write(
        library.join("skills/how/SKILL.md"),
        "---\nname: how\ndescription: \"Trace behavior\"\n---\n\n# how\n",
    )
    .unwrap();
    std::fs::write(
        library.join("skills/catalog.json"),
        r#"{"skills":[{"name":"how","category":"Test"}]}"#,
    )
    .unwrap();
    std::fs::write(library.join("AGENTS.md"), "shared policy\n").unwrap();
    std::fs::write(library.join(".gitignore"), "repo/\n").unwrap();
    fixtures::git(&library, &["init", "-q", "-b", "main"]);
    fixtures::git(&library, &["add", "."]);
    fixtures::git(&library, &["commit", "-q", "-m", "library"]);
    app.core
        .workspace
        .setup(WorkspaceSetup {
            url: None,
            adopt_path: Some(library.to_string_lossy().into_owned()),
        })
        .await
        .unwrap();

    let proposal_id = fixtures::skill_proposal(&app.core, &repo, "src", &["esc-one"]);
    let stored = stored_proposal(&app, &proposal_id);
    let path = format!("{PROPOSALS}/{proposal_id}/approve");
    let (status, body) = write(
        &app,
        "POST",
        &path,
        json!({
            "text": "---\nname: how\ndescription: A template.\n---\n\n- One.\n",
            "rev": stored.rev,
        }),
    )
    .await;
    assert_eq!((status, &body["code"]), (409, &json!("name_taken")));
    assert!(
        body["message"]
            .as_str()
            .is_some_and(|message| message.contains("shared library")),
        "{body}"
    );

    app.core
        .skills
        .write_local_skill(LocalSkillWrite::create(
            "---\nname: crew-escape-src\ndescription: Existing.\n---\n\n- One.\n",
        ))
        .unwrap();
    let (status, body) = write(
        &app,
        "POST",
        &path,
        json!({ "text": stored.text, "rev": stored.rev }),
    )
    .await;
    assert_eq!((status, &body["code"]), (409, &json!("name_taken")));
    assert!(
        body["message"]
            .as_str()
            .is_some_and(|message| message.contains("this device")),
        "{body}"
    );
    assert_eq!(stored_proposal(&app, &proposal_id).state, "open");
}

#[tokio::test]
async fn dismissing_keeps_the_row_and_refuses_a_second_or_stale_attempt() {
    let app = serve("proposals-dismiss", true, false).await;
    let repo = project(&app, "dismiss");
    let proposal_id = fixtures::skill_proposal(&app.core, &repo, "src", &["esc-one"]);
    let stored = stored_proposal(&app, &proposal_id);
    let path = format!("{PROPOSALS}/{proposal_id}/dismiss");

    let (status, dismissed) = write(&app, "POST", &path, json!({ "rev": stored.rev })).await;
    assert_eq!(status, 200, "{dismissed}");
    assert_eq!(dismissed["state"], "dismissed");
    assert_eq!(dismissed["rev"], 2);
    assert_eq!(dismissed["can_approve"], false);
    assert_eq!(dismissed["skill"], Value::Null);
    assert_eq!(dismissed["approved_at"], Value::Null);
    assert!(audited(&app, "crew.proposal.dismiss", &proposal_id));

    let (status, body) = write(&app, "POST", &path, json!({ "rev": 2 })).await;
    assert_eq!((status, &body["code"]), (409, &json!("not_open")));
    let (status, body) = write(
        &app,
        "POST",
        &format!("{PROPOSALS}/{proposal_id}/approve"),
        json!({ "text": stored.text, "rev": 2 }),
    )
    .await;
    assert_eq!((status, &body["code"]), (409, &json!("not_open")));

    // The dismissed row stays in the read, and the same pattern is not proposed again.
    let (status, list) = get(&app, &format!("{PROPOSALS}?repo={repo}")).await;
    assert_eq!(status, 200);
    assert_eq!(list["proposals"][0]["state"], "dismissed");

    let second = fixtures::skill_proposal(&app.core, &repo, "tests", &["esc-two"]);
    let (status, body) = write(
        &app,
        "POST",
        &format!("{PROPOSALS}/{second}/dismiss"),
        json!({ "rev": stored_proposal(&app, &second).rev + 9 }),
    )
    .await;
    assert_eq!((status, &body["code"]), (409, &json!("stale")));
    let (status, body) = write(
        &app,
        "POST",
        &format!("{PROPOSALS}/prop_missing/dismiss"),
        json!({ "rev": 1 }),
    )
    .await;
    assert_eq!((status, &body["code"]), (404, &json!("not_found")));
}

#[tokio::test]
async fn a_write_without_the_person_key_answers_desktop_required() {
    // No development override and no launch key: the service has no desktop shell, so both writes refuse.
    let app = serve_with(
        "proposals-key",
        true,
        false,
        Access::new("token", vec![], None),
    )
    .await;
    let repo = project(&app, "key");
    let proposal_id = fixtures::skill_proposal(&app.core, &repo, "src", &["esc-one"]);
    let stored = stored_proposal(&app, &proposal_id);
    let before = app.core.store.list_audit(None, 500).unwrap().len();
    for (path, body) in [
        (
            format!("{PROPOSALS}/{proposal_id}/approve"),
            json!({ "text": stored.text, "rev": stored.rev }),
        ),
        (
            format!("{PROPOSALS}/{proposal_id}/dismiss"),
            json!({ "rev": stored.rev }),
        ),
    ] {
        let (status, reply) = call(&app, "POST", &path, Some(body), None).await;
        assert_eq!(
            (status, &reply["code"]),
            (403, &json!("desktop_required")),
            "{path}"
        );
    }
    let unchanged = stored_proposal(&app, &proposal_id);
    assert_eq!((unchanged.state.as_str(), unchanged.rev), ("open", 1));
    assert_eq!(
        app.core.store.list_audit(None, 500).unwrap().len(),
        before,
        "a refused write writes no audit row"
    );
}

#[tokio::test]
async fn with_preview_off_every_proposal_route_is_not_found() {
    let app = serve("proposals-off", false, false).await;
    let (_, unknown) = get(&app, "/api/crew/not-a-route").await;
    let routes = [
        ("GET", format!("{PROPOSALS}?repo=p"), None),
        (
            "POST",
            format!("{PROPOSALS}/prop_one/approve"),
            Some(json!({})),
        ),
        (
            "POST",
            format!("{PROPOSALS}/prop_one/dismiss"),
            Some(json!({})),
        ),
    ];
    for (method, path, body) in routes {
        let (status, answer) = call(&app, method, &path, body, None).await;
        assert_eq!(
            (status, &answer["code"]),
            (404, &json!("not_found")),
            "{method} {path}"
        );
        assert_eq!(
            answer, unknown,
            "{method} {path} answers as an absent route does"
        );
    }
}
