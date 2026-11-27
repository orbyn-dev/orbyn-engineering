//! The project issue-tracker binding at the real HTTP edge (docs/issues.md, "Create, comment, react, templates and
//! project binding"): a project stores one connected tracker and scope, answers the binding, unbinds with `204`
//! idempotently, and writes one audit row with names only, never a token.

mod support;

use std::path::Path;

use serde_json::{Value, json};
use support::{Options, Service, TempDir, run_git};

fn repo(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
    run_git(path, &["init", "-q", "-b", "main"]);
    run_git(path, &["config", "user.email", "t@example.com"]);
    run_git(path, &["config", "user.name", "T"]);
    std::fs::write(path.join("a.txt"), "one\n").unwrap();
    run_git(path, &["add", "."]);
    run_git(path, &["commit", "-q", "-m", "first"]);
    run_git(
        path,
        &["remote", "add", "origin", "https://github.com/acme/app.git"],
    );
}

fn options_with_issues(dir: &TempDir) -> Options {
    let mut options = options(dir);
    options.env.push(("ORBYN_ISSUES".into(), "1".into()));
    options
}

fn options(dir: &TempDir) -> Options {
    Options {
        args: vec![],
        env: vec![(
            "PATH".into(),
            format!(
                "{}:{}",
                dir.path().join("bin").display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )],
    }
}

/// A stand-in `gh` so GitHub counts as connected; it is never run by the binding.
#[cfg(unix)]
fn stand_in_gh(dir: &TempDir) {
    use std::os::unix::fs::PermissionsExt;
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("gh"), "#!/bin/sh\nexit 1\n").unwrap();
    std::fs::set_permissions(bin.join("gh"), std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(not(unix))]
fn stand_in_gh(_dir: &TempDir) {}

/// A service with one trusted Git project whose origin names `acme/app`.
async fn fixture() -> (TempDir, Service, String) {
    let dir = TempDir::new();
    stand_in_gh(&dir);
    let service = Service::start(&dir.path().join("data"), options_with_issues(&dir));
    let folder = dir.path().join("app");
    repo(&folder);
    let project = service.project(&folder, true).await;
    (dir, service, project)
}

fn bindings(events: &[Value]) -> Vec<&Value> {
    events
        .iter()
        .filter(|event| event["action"] == "issues.project.bind")
        .collect()
}

async fn audit(service: &Service) -> Vec<Value> {
    service
        .json("/api/audit?limit=200")
        .await
        .as_array()
        .cloned()
        .unwrap_or_default()
}

#[cfg(unix)]
#[tokio::test]
async fn a_binding_is_answered_and_unbound_idempotently_with_one_audit_row() {
    let (_dir, service, project) = fixture().await;
    let path = format!("/api/projects/{project}/issue-tracker");
    let (status, body) = service
        .send(&path, json!({ "tracker": "github", "scope": "acme/app" }))
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, json!({ "tracker": "github", "scope": "acme/app" }));

    let events = audit(&service).await;
    let rows = bindings(&events);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["target"], json!(project));
    assert_eq!(
        rows[0]["detail"],
        json!({ "project": project, "tracker": "github", "scope": "acme/app" })
    );
    let detail = rows[0]["detail"].as_object().unwrap();
    assert_eq!(
        detail.keys().collect::<Vec<_>>(),
        ["project", "scope", "tracker"],
        "ids and names only: no token, path or other detail"
    );
    let stored = serde_json::to_string(&rows[0]).unwrap();
    assert!(!stored.to_lowercase().contains("token"), "{stored}");

    let (status, body) = service.send(&path, json!({ "tracker": null })).await;
    assert_eq!(status, 204, "{body}");
    // The desired end state is "no binding": a second unbind answers the same way.
    let (status, body) = service.send(&path, json!({ "tracker": null })).await;
    assert_eq!(status, 204, "{body}");
    assert_eq!(
        bindings(&audit(&service).await).len(),
        1,
        "an unbind writes no second audit row"
    );
}

#[tokio::test]
async fn an_unconnected_tracker_is_400_and_nothing_is_written() {
    let (_dir, service, project) = fixture().await;
    let path = format!("/api/projects/{project}/issue-tracker");
    let (status, body) = service
        .send(&path, json!({ "tracker": "gitlab", "scope": "grp/proj" }))
        .await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["code"], "invalid_input");
    assert!(
        bindings(&audit(&service).await).is_empty(),
        "a refused binding writes no audit row"
    );
    // Nothing changed: the project's reading side has no binding to answer with; a later unbind is still a no-op.
    let (status, _) = service.send(&path, json!({ "tracker": null })).await;
    assert_eq!(status, 204);
}

#[cfg(unix)]
#[tokio::test]
async fn a_scope_outside_the_connection_is_400_and_nothing_is_written() {
    let (_dir, service, project) = fixture().await;
    let path = format!("/api/projects/{project}/issue-tracker");
    let (status, body) = service
        .send(&path, json!({ "tracker": "github", "scope": "other/repo" }))
        .await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["code"], "invalid_input");
    assert!(bindings(&audit(&service).await).is_empty());
}

/// The table the binding lives in, from the service's own database: the one way a test can see that a refused call
/// stored nothing.
fn stored_bindings(dir: &TempDir) -> i64 {
    rusqlite::Connection::open(dir.path().join("data/orbyn.sqlite3"))
        .unwrap()
        .query_row("SELECT COUNT(*) FROM project_issue_trackers", [], |row| {
            row.get(0)
        })
        .unwrap()
}

#[tokio::test]
async fn the_route_is_behind_the_issues_flag_and_stores_nothing_when_it_is_off() {
    let dir = TempDir::new();
    stand_in_gh(&dir);
    let service = Service::start(&dir.path().join("data"), options(&dir));
    let folder = dir.path().join("app");
    repo(&folder);
    let project = service.project(&folder, true).await;
    let path = format!("/api/projects/{project}/issue-tracker");
    let (status, body) = service
        .send(&path, json!({ "tracker": "github", "scope": "acme/app" }))
        .await;
    assert_eq!(status, 404, "the issues flag is off: {body}");
    assert_eq!(body["code"], "not_found");
    assert_eq!(stored_bindings(&dir), 0, "nothing was stored");
    assert!(bindings(&audit(&service).await).is_empty());
    let (status, _) = service.send(&path, json!({ "tracker": null })).await;
    assert_eq!(status, 404, "the unbind is behind the flag too");
}

#[tokio::test]
async fn invalid_bodies_and_an_unknown_project_are_refused() {
    let (_dir, service, project) = fixture().await;
    let path = format!("/api/projects/{project}/issue-tracker");
    let (status, _) = service
        .send(&path, json!({ "tracker": null, "scope": "acme/app" }))
        .await;
    assert_eq!(status, 400, "an unbind names no scope");
    let (status, _) = service.send(&path, json!({ "tracker": "github" })).await;
    assert_eq!(status, 400, "a binding needs a scope");
    // A body that does not name the tracker must not unbind by accident.
    for body in [json!({}), json!({ "scope": "acme/app" })] {
        let (status, refusal) = service.send(&path, body.clone()).await;
        assert_eq!(
            status, 400,
            "a body without the tracker is refused: {body} {refusal}"
        );
        assert_eq!(refusal["code"], "invalid_input");
    }
    let (status, body) = service.send(&path, json!({ "tracker": null })).await;
    assert_eq!(status, 204, "an explicit null unbinds: {body}");
    let (status, _) = service
        .send(
            "/api/projects/not-a-project/issue-tracker",
            json!({ "tracker": "github", "scope": "acme/app" }),
        )
        .await;
    assert_eq!(status, 404, "the project must exist");
    assert!(bindings(&audit(&service).await).is_empty());
}
