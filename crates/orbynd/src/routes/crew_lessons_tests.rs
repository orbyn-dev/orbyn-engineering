//! Crew lessons over HTTP (KC5 slice 3, docs/crew-lessons.md): the list's filters and counts, one lesson with its
//! evidence, add by hand, edit, on and off, promote and demote, the 10 s delete and undo, and the per-repository switch.
//! Every write is a Person route that needs the person key, every read is Local, and with Preview off every route answers
//! as a route that is not there. The store and core rules are tested in `orbyn-core` (`store/crew_lessons_tests.rs`,
//! `crew/tests/lessons.rs`); this file only observes them over HTTP. The table, credential, remote and flag-off walks of
//! `crew_tests.rs` cover these routes too, since they sit under `/api/crew`.

use orbyn_core::crew::fixtures;
use orbyn_core::store::{LessonEvidence, NewCrewLesson};
use serde_json::{Value, json};

use super::crew_tasks_tests::{Served, serve, serve_with};
use crate::http::access::Access;

const LESSONS: &str = "/api/crew/lessons";
const SECRET_REFUSAL: &str =
    "Not saved: this looks like it holds a secret. Edit it and save again.";

/// One project row for the list, detail and settings routes.
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

/// A person's lesson by hand over HTTP; the answer must be the 201 the contract gives.
async fn add(app: &Served, scope: &str, repo: Option<&str>, text: &str) -> Value {
    let mut body = json!({ "scope": scope, "text": text });
    if let Some(repo) = repo {
        body["repo"] = json!(repo);
    }
    let (status, lesson) = write(app, "POST", LESSONS, body).await;
    assert_eq!(status, 201, "{lesson}");
    lesson
}

fn id(lesson: &Value) -> String {
    lesson["id"].as_str().expect("a lesson id").to_owned()
}

fn rev(lesson: &Value) -> u64 {
    lesson["rev"].as_u64().expect("a lesson rev")
}

async fn detail(app: &Served, id: &str) -> (u16, Value) {
    get(app, &format!("{LESSONS}/{id}")).await
}

/// One lesson written straight to the store, for a state no route should need many calls to build.
fn seed(app: &Served, scope: &str, repo: Option<&str>, text: &str) -> String {
    app.core
        .store
        .write_crew_lesson(NewCrewLesson {
            scope: scope.to_owned(),
            project_id: repo.map(str::to_owned),
            source: "person".to_owned(),
            text: text.to_owned(),
            fingerprint: None,
            evidence: vec![LessonEvidence {
                kind: "task".to_owned(),
                id: "sha256:one".to_owned(),
            }],
            edited_by_person: true,
            names_path: false,
        })
        .unwrap()
        .expect("a lesson with no fingerprint is written")
        .id
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
async fn the_list_filters_counts_and_includes_global_lessons() {
    let app = serve("lessons-list", true, false).await;
    let repo = project(&app, "list");
    let (status, empty) = get(&app, &format!("{LESSONS}?repo={repo}")).await;
    assert_eq!(status, 200);
    assert_eq!(empty["repo"], repo.as_str());
    assert_eq!(empty["use_lessons"], true);
    assert_eq!(empty["brief_cap"], json!({ "lessons": 5, "chars": 1500 }));
    assert_eq!(
        empty["counts"],
        json!({ "active": 0, "disabled": 0, "hidden": 0 })
    );
    assert_eq!(empty["lessons"], json!([]));

    let repo_lesson = add(&app, "repo", Some(&repo), "Keep each change reviewable.").await;
    let global = add(&app, "global", None, "Checks on this device are data only.").await;
    let disabled = add(&app, "repo", Some(&repo), "This lesson is turned off.").await;
    let (status, off) = write(
        &app,
        "POST",
        &format!("{LESSONS}/{}/disable", id(&disabled)),
        json!({ "rev": rev(&disabled) }),
    )
    .await;
    assert_eq!((status, off["state"].as_str()), (200, Some("disabled")));

    // Active is the default filter: the repository lesson, the global one, and not the disabled one.
    let (status, active) = get(&app, &format!("{LESSONS}?repo={repo}")).await;
    assert_eq!(status, 200);
    assert_eq!(
        active["counts"],
        json!({ "active": 2, "disabled": 1, "hidden": 0 })
    );
    let lessons = active["lessons"].as_array().unwrap();
    assert_eq!(lessons.len(), 2);
    assert_eq!(lessons[0]["id"], repo_lesson["id"]);
    assert_eq!(lessons[1]["id"], global["id"], "a global lesson is listed");
    assert_eq!(lessons[1]["scope"], "global");
    assert_eq!(lessons[1]["repo"], Value::Null);
    for lesson in lessons {
        assert_eq!(lesson["in_next_brief"], true, "{lesson}");
    }

    let (status, disabled_list) =
        get(&app, &format!("{LESSONS}?repo={repo}&filter=disabled")).await;
    assert_eq!(status, 200);
    assert_eq!(
        disabled_list["lessons"].as_array().unwrap().len(),
        1,
        "{disabled_list}"
    );
    assert_eq!(
        disabled_list["lessons"][0]["id"], disabled["id"],
        "{disabled_list}"
    );
    let (status, hidden_list) = get(&app, &format!("{LESSONS}?repo={repo}&filter=hidden")).await;
    assert_eq!(status, 200);
    assert_eq!(hidden_list["lessons"], json!([]));

    let (status, all) = get(&app, &format!("{LESSONS}?repo={repo}&filter=all")).await;
    assert_eq!(status, 200);
    assert_eq!(all["lessons"].as_array().unwrap().len(), 3);
    assert_eq!(
        all["counts"],
        json!({ "active": 2, "disabled": 1, "hidden": 0 })
    );

    let (status, body) = get(&app, &format!("{LESSONS}?repo={repo}&filter=sometimes")).await;
    assert_eq!((status, &body["code"]), (400, &json!("invalid")));
    let (status, body) = get(&app, LESSONS).await;
    assert_eq!((status, &body["code"]), (400, &json!("invalid")));
    let (status, body) = get(&app, &format!("{LESSONS}?repo=nobody")).await;
    assert_eq!((status, &body["code"]), (404, &json!("not_found")));
}

#[tokio::test]
async fn the_hidden_filter_lists_over_cap_lessons_and_the_counts_keep_them_apart() {
    let app = serve("lessons-hidden", true, false).await;
    let repo = project(&app, "hidden");
    for n in 0..50 {
        seed(
            &app,
            "repo",
            Some(&repo),
            &format!("The lesson numbered {n}."),
        );
    }
    let added = add(&app, "repo", Some(&repo), "The fifty first lesson.").await;

    let (status, hidden) = get(&app, &format!("{LESSONS}?repo={repo}&filter=hidden")).await;
    assert_eq!(status, 200);
    assert_eq!(
        hidden["counts"],
        json!({ "active": 50, "disabled": 0, "hidden": 1 })
    );
    let over = hidden["lessons"].as_array().unwrap();
    assert_eq!(over.len(), 1);
    assert_eq!(over[0]["hidden_by_cap"], true);
    assert_eq!(over[0]["in_next_brief"], false);
    assert_ne!(
        over[0]["id"], added["id"],
        "the row just written is visible"
    );

    let (status, active) = get(&app, &format!("{LESSONS}?repo={repo}")).await;
    assert_eq!(status, 200);
    assert_eq!(active["lessons"].as_array().unwrap().len(), 50);
    let (status, all) = get(&app, &format!("{LESSONS}?repo={repo}&filter=all")).await;
    assert_eq!(status, 200);
    assert_eq!(all["lessons"].as_array().unwrap().len(), 51);
}

#[tokio::test]
async fn a_global_lesson_detail_answers_for_the_repo_named_and_false_without_one() {
    let app = serve("lessons-detail", true, false).await;
    let repo = project(&app, "detail");
    let global = add(&app, "global", None, "The device keeps its lessons local.").await;
    let (status, with_repo) = get(&app, &format!("{LESSONS}/{}?repo={repo}", id(&global))).await;
    assert_eq!(status, 200);
    assert_eq!(with_repo["lesson"]["scope"], "global");
    assert_eq!(with_repo["lesson"]["repo"], Value::Null);
    assert_eq!(with_repo["lesson"]["in_next_brief"], true);
    assert_eq!(with_repo["evidence"], json!([]));

    let (status, without_repo) = detail(&app, &id(&global)).await;
    assert_eq!(status, 200);
    assert_eq!(
        without_repo["lesson"]["in_next_brief"], false,
        "a global lesson without a repo cannot answer"
    );
    let (status, body) = get(&app, &format!("{LESSONS}/{}?repo=nobody", id(&global))).await;
    assert_eq!((status, &body["code"]), (404, &json!("not_found")));

    // A repository lesson ignores `repo`, even one that is not a project.
    let repo_lesson = add(&app, "repo", Some(&repo), "A repository lesson.").await;
    let (status, answer) = get(&app, &format!("{LESSONS}/{}?repo=nobody", id(&repo_lesson))).await;
    assert_eq!(status, 200);
    assert_eq!(answer["lesson"]["repo"], repo.as_str());
    assert_eq!(answer["lesson"]["in_next_brief"], true);

    let (status, body) = detail(&app, "les_missing").await;
    assert_eq!((status, &body["code"]), (404, &json!("not_found")));
}

#[tokio::test]
async fn adding_a_lesson_answers_201_and_refuses_bad_text_or_scope() {
    let app = serve("lessons-add", true, false).await;
    let repo = project(&app, "add");
    let (status, lesson) = write(
        &app,
        "POST",
        LESSONS,
        json!({ "scope": "repo", "repo": repo, "text": "Checks run before a merge." }),
    )
    .await;
    assert_eq!(status, 201);
    assert_eq!(
        (
            lesson["scope"].as_str(),
            lesson["repo"].as_str(),
            lesson["source"].as_str()
        ),
        (Some("repo"), Some(repo.as_str()), Some("person"))
    );
    let lesson_id = id(&lesson);
    assert_eq!(rev(&lesson), 1);
    assert_eq!(lesson["state"], "active");
    assert_eq!(lesson["hidden_by_cap"], false);
    assert_eq!(lesson["names_path"], false);
    assert_eq!(lesson["edited_by_person"], true);
    assert_eq!(
        (lesson["uses"].as_u64(), lesson["left_out_of"].as_u64()),
        (Some(0), Some(0))
    );
    assert_eq!(lesson["last_used_at"], Value::Null);
    assert_eq!(lesson["in_next_brief"], true);
    assert!(audited(&app, "crew.lesson.learn", &lesson_id));

    let (status, global) = write(
        &app,
        "POST",
        LESSONS,
        json!({ "scope": "global", "text": "The device keeps its lessons local." }),
    )
    .await;
    assert_eq!(status, 201);
    assert_eq!(global["scope"], "global");
    assert_eq!(global["repo"], Value::Null);

    for (body, want) in [
        (
            json!({ "scope": "repo", "repo": repo, "text": "" }),
            json!("invalid"),
        ),
        (
            json!({ "scope": "repo", "repo": repo, "text": "a".repeat(301) }),
            json!("invalid"),
        ),
        (
            json!({ "scope": "repo", "text": "No repository." }),
            json!("invalid"),
        ),
        (
            json!({ "scope": "global", "text": "Look at src/app.rs before merging." }),
            json!("invalid"),
        ),
        (
            json!({ "scope": "repo", "repo": "nobody", "text": "An unknown repository." }),
            json!("not_found"),
        ),
        (
            json!({ "scope": "global", "text": "A lesson.", "extra": true }),
            json!("invalid_input"),
        ),
    ] {
        let (status, answer) = write(&app, "POST", LESSONS, body.clone()).await;
        let want_status = if want == json!("not_found") { 404 } else { 400 };
        assert_eq!(status, want_status, "{body}: {answer}");
        assert_eq!(answer["code"], want, "{body}: {answer}");
    }

    let secret = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
    let (status, refused) = write(
        &app,
        "POST",
        LESSONS,
        json!({ "scope": "repo", "repo": repo, "text": format!("A token {secret}") }),
    )
    .await;
    assert_eq!(status, 400, "{refused}");
    assert_eq!(refused["code"], "secret_refused");
    assert_eq!(refused["message"], SECRET_REFUSAL);

    // Only the two lessons that were accepted exist, and no refused word was stored.
    let (_, list) = get(&app, &format!("{LESSONS}?repo={repo}&filter=all")).await;
    assert_eq!(list["lessons"].as_array().unwrap().len(), 2);
    assert!(!list.to_string().contains(secret));
}

#[tokio::test]
async fn editing_and_switching_a_lesson_refuse_a_stale_rev_and_bump_it() {
    let app = serve("lessons-edit", true, false).await;
    let repo = project(&app, "edit");
    let lesson = add(&app, "repo", Some(&repo), "Keep each change small.").await;
    let lesson_id = id(&lesson);

    let (status, body) = write(
        &app,
        "PATCH",
        &format!("{LESSONS}/{lesson_id}"),
        json!({ "text": "A stale screen.", "rev": 999 }),
    )
    .await;
    assert_eq!((status, &body["code"]), (409, &json!("stale")));
    let (_, unchanged) = detail(&app, &lesson_id).await;
    assert_eq!(unchanged["lesson"]["text"], "Keep each change small.");
    assert_eq!(rev(&unchanged["lesson"]), 1);

    let (status, edited) = write(
        &app,
        "PATCH",
        &format!("{LESSONS}/{lesson_id}"),
        json!({ "text": "Keep each change reviewable.", "rev": 1 }),
    )
    .await;
    assert_eq!(status, 200, "{edited}");
    assert_eq!(edited["text"], "Keep each change reviewable.");
    assert_eq!(rev(&edited), 2);
    assert_eq!(edited["edited_by_person"], true);
    assert!(audited(&app, "crew.lesson.edit", &lesson_id));

    let (status, body) = write(
        &app,
        "POST",
        &format!("{LESSONS}/{lesson_id}/disable"),
        json!({ "rev": 1 }),
    )
    .await;
    assert_eq!((status, &body["code"]), (409, &json!("stale")));
    let (_, unchanged) = detail(&app, &lesson_id).await;
    assert_eq!(
        (
            unchanged["lesson"]["state"].as_str(),
            rev(&unchanged["lesson"])
        ),
        (Some("active"), 2)
    );

    let (status, disabled) = write(
        &app,
        "POST",
        &format!("{LESSONS}/{lesson_id}/disable"),
        json!({ "rev": 2 }),
    )
    .await;
    assert_eq!(status, 200, "{disabled}");
    assert_eq!(
        (disabled["state"].as_str(), rev(&disabled)),
        (Some("disabled"), 3)
    );
    assert!(audited(&app, "crew.lesson.disable", &lesson_id));

    let (status, body) = write(
        &app,
        "POST",
        &format!("{LESSONS}/{lesson_id}/enable"),
        json!({ "rev": 1 }),
    )
    .await;
    assert_eq!((status, &body["code"]), (409, &json!("stale")));
    let (_, unchanged) = detail(&app, &lesson_id).await;
    assert_eq!(unchanged["lesson"]["state"], "disabled");

    let (status, enabled) = write(
        &app,
        "POST",
        &format!("{LESSONS}/{lesson_id}/enable"),
        json!({ "rev": 3 }),
    )
    .await;
    assert_eq!(status, 200, "{enabled}");
    assert_eq!(
        (enabled["state"].as_str(), rev(&enabled)),
        (Some("active"), 4)
    );
    assert!(audited(&app, "crew.lesson.enable", &lesson_id));

    let (status, body) = write(
        &app,
        "PATCH",
        "/api/crew/lessons/les_missing",
        json!({ "text": "No such lesson.", "rev": 1 }),
    )
    .await;
    assert_eq!((status, &body["code"]), (404, &json!("not_found")));
    let (status, body) = write(
        &app,
        "PATCH",
        &format!("{LESSONS}/{lesson_id}"),
        json!({ "text": "A lesson.", "rev": 4, "extra": true }),
    )
    .await;
    assert_eq!((status, &body["code"]), (400, &json!("invalid_input")));
}

#[tokio::test]
async fn promoting_and_demoting_refuse_a_stale_rev_and_a_path_named_lesson() {
    let app = serve("lessons-scope", true, false).await;
    let repo = project(&app, "scope");
    let lesson = add(&app, "repo", Some(&repo), "Checks run before a merge.").await;
    let lesson_id = id(&lesson);

    let (status, body) = write(
        &app,
        "POST",
        &format!("{LESSONS}/{lesson_id}/promote"),
        json!({ "rev": 999 }),
    )
    .await;
    assert_eq!((status, &body["code"]), (409, &json!("stale")));
    let (_, unchanged) = detail(&app, &lesson_id).await;
    assert_eq!(
        (
            unchanged["lesson"]["scope"].as_str(),
            rev(&unchanged["lesson"])
        ),
        (Some("repo"), 1)
    );

    let (status, promoted) = write(
        &app,
        "POST",
        &format!("{LESSONS}/{lesson_id}/promote"),
        json!({ "rev": 1 }),
    )
    .await;
    assert_eq!(status, 200, "{promoted}");
    assert_eq!(
        (
            promoted["scope"].as_str(),
            promoted["repo"].clone(),
            rev(&promoted)
        ),
        (Some("global"), Value::Null, 2)
    );
    assert!(audited(&app, "crew.lesson.promote", &lesson_id));

    let (status, body) = write(
        &app,
        "POST",
        &format!("{LESSONS}/{lesson_id}/demote"),
        json!({ "repo": repo, "rev": 1 }),
    )
    .await;
    assert_eq!((status, &body["code"]), (409, &json!("stale")));
    let (_, unchanged) = detail(&app, &lesson_id).await;
    assert_eq!(unchanged["lesson"]["scope"], "global");

    let (status, demoted) = write(
        &app,
        "POST",
        &format!("{LESSONS}/{lesson_id}/demote"),
        json!({ "repo": repo, "rev": 2 }),
    )
    .await;
    assert_eq!(status, 200, "{demoted}");
    assert_eq!(
        (
            demoted["scope"].as_str(),
            demoted["repo"].as_str(),
            rev(&demoted)
        ),
        (Some("repo"), Some(repo.as_str()), 3)
    );
    assert!(audited(&app, "crew.lesson.demote", &lesson_id));
    let (status, body) = write(
        &app,
        "POST",
        &format!("{LESSONS}/{lesson_id}/demote"),
        json!({ "repo": "nobody", "rev": 3 }),
    )
    .await;
    assert_eq!((status, &body["code"]), (404, &json!("not_found")));

    // A repository lesson may name a path; promoting it is refused until it is edited.
    let named = add(
        &app,
        "repo",
        Some(&repo),
        "Look at src/app.rs before merging.",
    )
    .await;
    assert_eq!(named["names_path"], true);
    let (status, body) = write(
        &app,
        "POST",
        &format!("{LESSONS}/{}/promote", id(&named)),
        json!({ "rev": rev(&named) }),
    )
    .await;
    assert_eq!((status, &body["code"]), (400, &json!("invalid")));
    let (_, unchanged) = detail(&app, &id(&named)).await;
    assert_eq!(
        (
            unchanged["lesson"]["scope"].as_str(),
            rev(&unchanged["lesson"])
        ),
        (Some("repo"), 1)
    );
}

#[tokio::test]
async fn deleting_starts_the_undo_and_undo_restores_the_lesson() {
    let app = serve("lessons-delete", true, false).await;
    let repo = project(&app, "delete");
    let lesson = add(&app, "repo", Some(&repo), "A lesson to undo.").await;
    let lesson_id = id(&lesson);

    let (status, body) = call(
        &app,
        "DELETE",
        &format!("{LESSONS}/{lesson_id}?rev=999"),
        None,
        None,
    )
    .await;
    assert_eq!((status, &body["code"]), (409, &json!("stale")));
    let (status, _) = detail(&app, &lesson_id).await;
    assert_eq!(status, 200, "a stale delete writes nothing");

    let (status, deleted) = call(
        &app,
        "DELETE",
        &format!("{LESSONS}/{lesson_id}?rev=1"),
        None,
        None,
    )
    .await;
    assert_eq!(status, 200, "{deleted}");
    assert_eq!(deleted["id"], lesson_id.as_str());
    assert!(
        deleted["undo_until"]
            .as_str()
            .is_some_and(|at| at.ends_with('Z') && at.len() >= 20),
        "{deleted}"
    );
    assert!(audited(&app, "crew.lesson.delete", &lesson_id));
    let (status, body) = detail(&app, &lesson_id).await;
    assert_eq!((status, &body["code"]), (404, &json!("not_found")));
    let (_, list) = get(&app, &format!("{LESSONS}?repo={repo}&filter=all")).await;
    assert_eq!(list["lessons"], json!([]));

    let (status, restored) = call(
        &app,
        "POST",
        &format!("{LESSONS}/{lesson_id}/undo"),
        None,
        None,
    )
    .await;
    assert_eq!(status, 200, "{restored}");
    assert_eq!(
        (restored["id"].as_str(), rev(&restored)),
        (Some(lesson_id.as_str()), 2)
    );
    assert!(audited(&app, "crew.lesson.undo", &lesson_id));
    let (status, again) = detail(&app, &lesson_id).await;
    assert_eq!((status, rev(&again["lesson"])), (200, 2));

    let (status, body) = call(
        &app,
        "POST",
        "/api/crew/lessons/les_missing/undo",
        None,
        None,
    )
    .await;
    assert_eq!((status, &body["code"]), (404, &json!("not_found")));

    let (status, body) = call(
        &app,
        "DELETE",
        &format!("{LESSONS}/{lesson_id}"),
        None,
        None,
    )
    .await;
    assert_eq!((status, &body["code"]), (400, &json!("invalid")));
    let (status, body) = call(
        &app,
        "DELETE",
        &format!("{LESSONS}/{lesson_id}?rev=abc"),
        None,
        None,
    )
    .await;
    assert_eq!((status, &body["code"]), (400, &json!("invalid")));
}

#[tokio::test]
async fn the_settings_switch_flips_use_lessons_and_the_list_shows_it() {
    let app = serve("lessons-settings", true, false).await;
    let repo = project(&app, "settings");
    add(&app, "repo", Some(&repo), "A lesson behind the switch.").await;

    let (status, off) = write(
        &app,
        "PUT",
        &format!("{LESSONS}/settings"),
        json!({ "repo": repo, "use_lessons": false }),
    )
    .await;
    assert_eq!(status, 200, "{off}");
    assert_eq!(off, json!({ "repo": repo, "use_lessons": false }), "{off}");
    assert!(audited(&app, "crew.lesson.settings", &repo));
    let (_, list) = get(&app, &format!("{LESSONS}?repo={repo}")).await;
    assert_eq!(list["use_lessons"], false);
    for lesson in list["lessons"].as_array().unwrap() {
        assert_eq!(lesson["in_next_brief"], false, "{lesson}");
    }

    let (status, on) = write(
        &app,
        "PUT",
        &format!("{LESSONS}/settings"),
        json!({ "repo": repo, "use_lessons": true }),
    )
    .await;
    assert_eq!(status, 200, "{on}");
    assert_eq!(on["use_lessons"], true);
    let (_, list) = get(&app, &format!("{LESSONS}?repo={repo}")).await;
    assert_eq!(list["use_lessons"], true);
    assert_eq!(list["lessons"][0]["in_next_brief"], true);

    let (status, body) = write(
        &app,
        "PUT",
        &format!("{LESSONS}/settings"),
        json!({ "repo": "nobody", "use_lessons": false }),
    )
    .await;
    assert_eq!((status, &body["code"]), (404, &json!("not_found")));
    let (status, body) = write(
        &app,
        "PUT",
        &format!("{LESSONS}/settings"),
        json!({ "repo": repo, "use_lessons": false, "extra": true }),
    )
    .await;
    assert_eq!((status, &body["code"]), (400, &json!("invalid_input")));
}

#[tokio::test]
async fn a_write_without_the_person_key_answers_desktop_required() {
    // No development override and no launch key: the service has no desktop shell, so every enforced route refuses.
    let app = serve_with(
        "lessons-key",
        true,
        false,
        Access::new("token", vec![], None),
    )
    .await;
    let repo = project(&app, "key");
    let lesson_id = seed(&app, "repo", Some(&repo), "One stored lesson.");
    let before = app.core.store.list_audit(None, 500).unwrap().len();
    let writes = [
        (
            "POST",
            LESSONS.to_owned(),
            json!({ "scope": "repo", "repo": repo, "text": "By hand." }),
        ),
        (
            "PATCH",
            format!("{LESSONS}/{lesson_id}"),
            json!({ "text": "Edited.", "rev": 1 }),
        ),
        (
            "POST",
            format!("{LESSONS}/{lesson_id}/disable"),
            json!({ "rev": 1 }),
        ),
        (
            "POST",
            format!("{LESSONS}/{lesson_id}/enable"),
            json!({ "rev": 1 }),
        ),
        (
            "DELETE",
            format!("{LESSONS}/{lesson_id}?rev=1"),
            Value::Null,
        ),
        ("POST", format!("{LESSONS}/{lesson_id}/undo"), Value::Null),
        (
            "POST",
            format!("{LESSONS}/{lesson_id}/promote"),
            json!({ "rev": 1 }),
        ),
        (
            "POST",
            format!("{LESSONS}/{lesson_id}/demote"),
            json!({ "repo": repo, "rev": 1 }),
        ),
        (
            "PUT",
            format!("{LESSONS}/settings"),
            json!({ "repo": repo, "use_lessons": false }),
        ),
    ];
    for (method, path, body) in writes {
        let body = if body.is_null() { None } else { Some(body) };
        let (status, reply) = call(&app, method, &path, body, None).await;
        assert_eq!(
            (status, &reply["code"]),
            (403, &json!("desktop_required")),
            "{method} {path}"
        );
    }
    let stored = app.core.store.crew_lesson(&lesson_id).unwrap().unwrap();
    assert_eq!(
        (stored.rev, stored.state.as_str(), stored.scope.as_str()),
        (1, "active", "repo")
    );
    assert_eq!(
        app.core.store.list_audit(None, 500).unwrap().len(),
        before,
        "a refused write writes no audit row"
    );
}

#[tokio::test]
async fn with_preview_off_every_lesson_route_is_not_found() {
    let app = serve("lessons-off", false, false).await;
    let (_, unknown) = get(&app, "/api/crew/not-a-route").await;
    let routes = [
        ("GET", format!("{LESSONS}?repo=p"), None),
        ("GET", format!("{LESSONS}/les_one?repo=p"), None),
        ("POST", LESSONS.to_owned(), Some(json!({}))),
        ("PATCH", format!("{LESSONS}/les_one"), Some(json!({}))),
        ("DELETE", format!("{LESSONS}/les_one?rev=1"), None),
        (
            "POST",
            format!("{LESSONS}/les_one/disable"),
            Some(json!({})),
        ),
        ("POST", format!("{LESSONS}/les_one/enable"), Some(json!({}))),
        ("POST", format!("{LESSONS}/les_one/undo"), None),
        (
            "POST",
            format!("{LESSONS}/les_one/promote"),
            Some(json!({})),
        ),
        ("POST", format!("{LESSONS}/les_one/demote"), Some(json!({}))),
        ("PUT", format!("{LESSONS}/settings"), Some(json!({}))),
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
