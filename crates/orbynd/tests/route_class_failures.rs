//! Route-class failures at the real HTTP edge: a remote session with no desktop shell, a remote viewer's
//! `local_only` writes, a forged cookie and a foreign Host, the person layer's order against the Crew Preview gate,
//! and a positive control that a `local` route answers the service token alone.
//!
//! The service starts from the real binary through `support`, so every case goes through `require_auth`, `guard_origin`,
//! `require_person` and the route's own gate in the order production uses. The development person override is off for
//! the cases about the missing key and on for the local-only cases, exactly as the other HTTP tests separate the two.

mod support;

use std::path::Path;

use serde_json::{Value, json};
use support::{Options, Service, TempDir};

const PERSON_HEADER: &str = "x-orbyn-person";

struct RemoteSession {
    service: Service,
    cookie: String,
    _dir: TempDir,
}

/// A real remote service (`--remote --tls-terminated`) and the session cookie its first access key opens.
/// `open_person` sets the debug-only development override: on means a `Person` route reaches its own remote gate,
/// off means the service has no desktop shell and no key.
async fn remote_session(open_person: bool, allowed_hosts: &[&str]) -> RemoteSession {
    let dir = TempDir::new();
    let mut args = vec!["--remote".to_owned(), "--tls-terminated".to_owned()];
    for host in allowed_hosts {
        args.push("--allowed-host".to_owned());
        args.push((*host).to_owned());
    }
    let service = Service::start(
        &dir.path().join("data"),
        Options {
            args,
            env: vec![(
                "ORBYN_DEV_DISABLE_PERSON_KEY".to_owned(),
                if open_person { "1" } else { "0" }.to_owned(),
            )],
        },
    );
    let access_key = service
        .access_key
        .clone()
        .expect("the first remote start shows the access key");
    let response = service
        .client
        .post(format!("{}/auth/session", service.url))
        .json(&json!({ "key": access_key }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204, "the access key opens a session");
    let cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    RemoteSession {
        service,
        cookie,
        _dir: dir,
    }
}

/// One POST with a cookie session and no person key, as a remote viewer sends it.
async fn remote_post(remote: &RemoteSession, path: &str, body: Value) -> (u16, Value) {
    let response = remote
        .service
        .client
        .post(format!("{}{path}", remote.service.url))
        .header("Cookie", &remote.cookie)
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    (
        status,
        serde_json::from_str(&response.text().await.unwrap()).unwrap_or(Value::Null),
    )
}

/// A remote service with no desktop shell has no person key, so the two person writes that grant authority answer
/// `desktop_required` even to a valid session.
#[tokio::test]
async fn a_remote_session_without_a_desktop_shell_answers_desktop_required_on_person_writes() {
    let remote = remote_session(false, &[]).await;
    assert!(remote.service.person_key.is_none(), "no shell, no key");
    let info = remote
        .service
        .client
        .get(format!("{}/api/info", remote.service.url))
        .header("Cookie", &remote.cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(info.status(), 200, "the cookie opens the service");
    assert_eq!(info.json::<Value>().await.unwrap()["remote"], true);

    for (path, body) in [
        (
            "/api/approvals/approval-1/decide",
            json!({ "allow": true, "remember": false }),
        ),
        ("/api/projects/project-1/trust", json!({})),
    ] {
        let (status, reply) = remote_post(&remote, path, body).await;
        assert_eq!(
            (status, reply["code"].as_str()),
            (403, Some("desktop_required")),
            "{path}"
        );
    }
}

/// A remote viewer cannot use the one-turn override or acknowledge an engine notice: both routes are `local_only`.
/// The development override is on, as in the other HTTP tests, so the person layer lets the cookie through and the
/// remote gate is what answers; the positive control is the same cookie reading `/api/info`.
#[tokio::test]
async fn a_remote_viewer_cannot_use_the_one_turn_override_or_acknowledge_a_notice() {
    let remote = remote_session(true, &[]).await;
    let info = remote
        .service
        .client
        .get(format!("{}/api/info", remote.service.url))
        .header("Cookie", &remote.cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(info.status(), 200, "the cookie opens the service");

    for (path, body) in [
        (
            "/api/threads/thread-1/send-without-approvals",
            json!({ "prompt": "go" }),
        ),
        (
            "/api/projects/project-1/engines/claude/notice/acknowledge",
            Value::Null,
        ),
    ] {
        let (status, reply) = remote_post(&remote, path, body).await;
        assert_eq!(
            (status, reply["code"].as_str()),
            (403, Some("local_only")),
            "{path}"
        );
    }
}

/// A forged session cookie is no credential, and a Host that `--allowed-host` did not name is refused before auth.
#[tokio::test]
async fn a_remote_service_refuses_a_forged_cookie_and_a_foreign_host() {
    let remote = remote_session(false, &["box.tailnet.ts.net"]).await;

    let forged = remote
        .service
        .client
        .get(format!("{}/api/info", remote.service.url))
        .header("Cookie", "orbyn_session=forged")
        .send()
        .await
        .unwrap();
    let status = forged.status().as_u16();
    let reply: Value = forged.json().await.unwrap();
    assert_eq!(
        (status, reply["code"].as_str()),
        (401, Some("unauthorized"))
    );

    let foreign = remote
        .service
        .client
        .get(format!("{}/api/info", remote.service.url))
        .bearer_auth(&remote.service.token)
        .header("Host", "evil.test")
        .send()
        .await
        .unwrap();
    let status = foreign.status().as_u16();
    let reply: Value = foreign.json().await.unwrap();
    assert_eq!(
        (status, reply["code"].as_str()),
        (403, Some("host_not_allowed"))
    );
}

/// The person layer is outside the Crew Preview gate: with the Preview flag off, a token-only merge answers the
/// person refusal (403 `person_required`), while the same call with the shell's key passes the person layer and is
/// answered by the Preview gate (404 `not_found`).
#[tokio::test]
async fn the_person_layer_answers_before_the_crew_preview_gate() {
    let dir = TempDir::new();
    let service = Service::start(
        &dir.path().join("data"),
        Options {
            args: vec!["--parent-stdin".into()],
            env: vec![
                ("ORBYN_DEV_DISABLE_PERSON_KEY".into(), "0".into()),
                ("ORBYN_CREW_PREVIEW".into(), "0".into()),
            ],
        },
    );
    let key = service.person_key.clone().expect("the shell gets a key");
    let info = service.json("/api/info").await;
    assert!(
        !info["crew_preview"].as_bool().unwrap_or(false),
        "the Preview gate is off: {info}"
    );

    let path = "/api/crew/tasks/task-1/merge";
    let body = json!({ "operation": "release" });
    let refused = service.post(path).json(&body).send().await.unwrap();
    let status = refused.status().as_u16();
    let reply: Value = refused.json().await.unwrap();
    assert_eq!(
        (status, reply["code"].as_str()),
        (403, Some("person_required")),
        "the person layer runs before the Preview gate"
    );

    let gated = service
        .post(path)
        .header(PERSON_HEADER, &key)
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = gated.status().as_u16();
    let reply: Value = gated.json().await.unwrap();
    assert_eq!(
        (status, reply["code"].as_str()),
        (404, Some("not_found")),
        "with the key, the Preview gate answers"
    );
    // The core's own Preview guard would answer "route not found": this exact text is the transport gate's.
    assert_eq!(reply["message"], json!("No such route."));
}

/// The class of a protected route as `tests/routes.json` pins it.
fn class_of(method: &str, path: &str) -> String {
    let text =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/routes.json"))
            .unwrap();
    let table: Value = serde_json::from_str(&text).unwrap();
    table["protected"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row[0] == method && row[1] == path)
        .map(|row| row[2].as_str().unwrap().to_owned())
        .unwrap_or_else(|| panic!("{method} {path} is not a protected route row"))
}

/// A positive control for the class split: a `local` GET, POST and DELETE each answer the service token alone, with no
/// person key and the development override off.
#[tokio::test]
async fn a_local_get_post_and_delete_answer_the_service_token_without_a_person_key() {
    let dir = TempDir::new();
    let service = Service::start(
        &dir.path().join("data"),
        Options {
            env: vec![("ORBYN_DEV_DISABLE_PERSON_KEY".into(), "0".into())],
            ..Options::default()
        },
    );
    assert!(service.person_key.is_none(), "no shell, no key");
    for (method, path) in [
        ("GET", "/api/info"),
        ("POST", "/api/browser-profiles"),
        ("DELETE", "/api/browser-profiles/{id}"),
    ] {
        assert_eq!(class_of(method, path), "local", "{method} {path}");
    }

    let info = service.get("/api/info").send().await.unwrap();
    assert_eq!(info.status(), 200, "a local GET");

    let created = service
        .post("/api/browser-profiles")
        .json(&json!({ "name": "audit-profile" }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201, "a local POST");
    let profile: Value = created.json().await.unwrap();
    let id = profile["id"].as_str().expect("a created profile id");

    let deleted = service
        .client
        .delete(format!("{}/api/browser-profiles/{id}", service.url))
        .bearer_auth(&service.token)
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), 204, "a local DELETE");
}
