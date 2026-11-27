//! The three issue writes at the real HTTP edge (docs/issues.md, "Create, comment, react"):
//! `POST /api/issues/item/create`, `/comment` and `/react`. Each is behind the Issues flag, is a Person
//! route, and refuses an unconnected tracker, a scope outside the connection and a body over the caps
//! with its own status and `code`, before anything is sent to a tracker.

mod support;

use serde_json::{Value, json};
use support::{Options, Service, TempDir};

const CREATE: &str = "/api/issues/item/create";
const COMMENT: &str = "/api/issues/item/comment";
const REACT: &str = "/api/issues/item/react";

/// One body per write that a connected tracker would accept, so a refusal below names the gate under
/// test rather than a body the route could not read.
fn writes(tracker: &str, scope: &str) -> Vec<(&'static str, Value)> {
    vec![
        (
            CREATE,
            json!({ "tracker": tracker, "scope": scope, "title": "Crash on start" }),
        ),
        (
            COMMENT,
            json!({ "tracker": tracker, "scope": scope, "key": "42", "body": "On it." }),
        ),
        (
            REACT,
            json!({
                "tracker": tracker, "scope": scope, "key": "42",
                "reaction": "thumbs_up", "on": true,
            }),
        ),
    ]
}

/// A stand-in `gh` so GitHub counts as connected. Every case that resolves it is refused before a
/// write, so it is never run.
#[cfg(unix)]
fn stand_in_gh(dir: &TempDir) {
    use std::os::unix::fs::PermissionsExt;
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("gh"), "#!/bin/sh\nexit 1\n").unwrap();
    std::fs::set_permissions(bin.join("gh"), std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A bare name resolves through `PATHEXT` on Windows, and a `.cmd` shim is followed to the script it
/// wraps, so the stand-in is a plain file called `gh.exe`: the service resolves it and runs nothing.
#[cfg(windows)]
fn stand_in_gh(dir: &TempDir) {
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("gh.exe"), "stand-in\n").unwrap();
}

/// A service whose PATH starts in the temp dir, so `stand_in_gh` alone decides whether GitHub is
/// connected, and whose Issues flag is the one asked for.
fn options(dir: &TempDir, issues: bool) -> Options {
    let mut paths = vec![dir.path().join("bin")];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path = std::env::join_paths(paths).unwrap();
    Options {
        args: Vec::new(),
        env: vec![
            ("PATH".into(), path.to_string_lossy().into_owned()),
            ("ORBYN_ISSUES".into(), if issues { "1" } else { "0" }.into()),
        ],
    }
}

/// A service with GitHub connected in the one scope `acme/app`.
async fn connected(dir: &TempDir) -> Service {
    stand_in_gh(dir);
    let service = Service::start(&dir.path().join("data"), options(dir, true));
    let (status, body) = service
        .send(
            "/api/issues/trackers/github/connect",
            json!({ "scope": "acme/app" }),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    service
}

#[tokio::test]
async fn every_write_is_behind_the_issues_flag() {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), options(&dir, false));
    for (path, body) in writes("github", "acme/app") {
        let (status, answer) = service.send(path, body).await;
        assert_eq!(status, 404, "{path}: {answer}");
        assert_eq!(answer["code"], "not_found", "{path}: {answer}");
    }
}

/// No desktop shell means no person key: the layer refuses the write in the shipped words and the
/// body is never read.
#[tokio::test]
async fn a_service_without_a_desktop_shell_answers_desktop_required() {
    let dir = TempDir::new();
    let mut options = options(&dir, true);
    options
        .env
        .push(("ORBYN_DEV_DISABLE_PERSON_KEY".into(), "0".into()));
    let service = Service::start(&dir.path().join("data"), options);
    assert!(service.person_key.is_none(), "no shell, no key");
    for (path, body) in writes("github", "acme/app") {
        let (status, answer) = service.send(path, body).await;
        assert_eq!(status, 403, "{path}: {answer}");
        assert_eq!(answer["code"], "desktop_required", "{path}: {answer}");
        assert_eq!(
            answer["message"], "This action is only available in the desktop app.",
            "{path}: {answer}"
        );
    }
}

/// A tracker with no connection answers the "not connected" refusal and nothing is written. GitLab
/// is used because GitHub's connection follows `gh` being on PATH, which a developer machine may have.
#[tokio::test]
async fn an_unconnected_tracker_is_409_tracker_not_connected() {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), options(&dir, true));
    for (path, body) in writes("gitlab", "grp/proj") {
        let (status, answer) = service.send(path, body).await;
        assert_eq!(status, 409, "{path}: {answer}");
        assert_eq!(answer["code"], "tracker_not_connected", "{path}: {answer}");
    }
}

/// A scope outside the connection is refused before anything is read or written.
#[tokio::test]
async fn a_scope_outside_the_connection_is_400_invalid_input() {
    let dir = TempDir::new();
    let service = connected(&dir).await;
    for (path, body) in writes("github", "other/repo") {
        let (status, answer) = service.send(path, body).await;
        assert_eq!(status, 400, "{path}: {answer}");
        assert_eq!(answer["code"], "invalid_input", "{path}: {answer}");
        assert_eq!(
            answer["message"], "That scope is not part of this connection.",
            "{path}: {answer}"
        );
    }
}

/// Every cap docs/issues.md gives the three routes, each refused with its own sentence before the
/// tracker is sent anything (the caps are checked ahead of the write).
#[tokio::test]
async fn a_body_over_the_caps_is_400_invalid_input_before_anything_is_sent() {
    let dir = TempDir::new();
    let service = connected(&dir).await;
    let long_title = "x".repeat(257);
    let long_label = "y".repeat(201);
    let big_body = "z".repeat(64 * 1024 + 1);
    let labels_51 = (0..51).map(|n| format!("label-{n}")).collect::<Vec<_>>();
    let assignees_26 = (0..26).map(|n| n.to_string()).collect::<Vec<_>>();
    let cases = [
        (
            CREATE,
            json!({ "tracker": "github", "scope": "acme/app", "title": "   " }),
            "The issue needs a title.",
        ),
        (
            CREATE,
            json!({ "tracker": "github", "scope": "acme/app", "title": long_title }),
            "The title is over 256 characters.",
        ),
        (
            CREATE,
            json!({ "tracker": "github", "scope": "acme/app", "title": "one\ntwo" }),
            "A title is one line.",
        ),
        (
            CREATE,
            json!({ "tracker": "github", "scope": "acme/app", "title": "Ok", "body": big_body }),
            "The issue body is over 64 KiB.",
        ),
        (
            CREATE,
            json!({ "tracker": "github", "scope": "acme/app", "title": "Ok", "labels": labels_51 }),
            "You can set at most 50 labels.",
        ),
        (
            CREATE,
            json!({ "tracker": "github", "scope": "acme/app", "title": "Ok", "labels": [long_label] }),
            "A label name is empty, too long or has control characters.",
        ),
        (
            CREATE,
            json!({ "tracker": "github", "scope": "acme/app", "title": "Ok", "labels": ["two\nlines"] }),
            "A label name is empty, too long or has control characters.",
        ),
        (
            CREATE,
            json!({ "tracker": "github", "scope": "acme/app", "title": "Ok", "assignees": assignees_26 }),
            "You can assign at most 25 people.",
        ),
        (
            CREATE,
            json!({ "tracker": "github", "scope": "acme/app", "title": "Ok", "assignees": [""] }),
            "That assignee id is not valid.",
        ),
        (
            COMMENT,
            json!({ "tracker": "github", "scope": "acme/app", "key": "42", "body": "   " }),
            "The comment is empty.",
        ),
        (
            COMMENT,
            json!({ "tracker": "github", "scope": "acme/app", "key": "42", "body": big_body }),
            "The comment is over 64 KiB.",
        ),
        (
            REACT,
            json!({
                "tracker": "github", "scope": "acme/app", "key": "42",
                "comment_id": "", "reaction": "heart", "on": true,
            }),
            "That comment id is not valid.",
        ),
        (
            REACT,
            json!({
                "tracker": "github", "scope": "acme/app", "key": "42",
                "comment_id": "c".repeat(201), "reaction": "heart", "on": true,
            }),
            "That comment id is not valid.",
        ),
        (
            REACT,
            json!({
                "tracker": "github", "scope": "acme/app", "key": "42",
                "comment_id": "a\nb", "reaction": "heart", "on": true,
            }),
            "That comment id is not valid.",
        ),
    ];
    for (path, body, message) in cases {
        let (status, answer) = service.send(path, body).await;
        assert_eq!(status, 400, "{path} {message}: {answer}");
        assert_eq!(
            answer["code"], "invalid_input",
            "{path} {message}: {answer}"
        );
        assert_eq!(answer["message"], message, "{path}: {answer}");
    }
}

/// A body the route cannot read is `400 invalid_input` naming the field and never the value sent.
#[tokio::test]
async fn a_body_of_the_wrong_shape_is_400_invalid_input_naming_the_field() {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), options(&dir, true));
    for (path, body, message) in [
        (
            CREATE,
            json!({ "tracker": "svn", "scope": "acme/app", "title": "Ok" }),
            "The request body is not valid: the field `tracker` has a value this route does not accept.",
        ),
        (
            CREATE,
            json!({ "tracker": "github", "scope": "acme/app", "title": 48190273 }),
            "The request body is not valid: the field `title` has a value this route does not accept.",
        ),
        (
            CREATE,
            json!({ "tracker": "github", "scope": "acme/app" }),
            "The request body is not valid: it does not match what this route expects.",
        ),
        (
            COMMENT,
            json!({ "tracker": "github", "scope": "acme/app", "key": 48190273, "body": "On it." }),
            "The request body is not valid: the field `key` has a value this route does not accept.",
        ),
        (
            REACT,
            json!({
                "tracker": "github", "scope": "acme/app", "key": "42",
                "reaction": "shrug", "on": true,
            }),
            "The request body is not valid: the field `reaction` has a value this route does not accept.",
        ),
    ] {
        let (status, answer) = service.send(path, body).await;
        assert_eq!(status, 400, "{path}: {answer}");
        assert_eq!(answer["code"], "invalid_input", "{path}: {answer}");
        assert_eq!(answer["message"], message, "{path}: {answer}");
        assert!(
            !answer.to_string().contains("48190273"),
            "the answer echoes a value: {answer}"
        );
    }
}

/// A body with no JSON content type is refused with the fixed sentence on every write route.
#[tokio::test]
async fn a_body_without_a_json_content_type_is_400_invalid_input() {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), options(&dir, true));
    for (path, body) in writes("github", "acme/app") {
        let response = service
            .post(path)
            .body(body.to_string())
            .send()
            .await
            .expect("the route answers");
        let status = response.status().as_u16();
        let answer: Value = response.json().await.unwrap_or(Value::Null);
        assert_eq!(status, 400, "{path}: {answer}");
        assert_eq!(answer["code"], "invalid_input", "{path}: {answer}");
        assert_eq!(
            answer["message"], "Expected request with `Content-Type: application/json`",
            "{path}: {answer}"
        );
    }
}
