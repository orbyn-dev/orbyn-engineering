//! The engine API-key routes: the person-only writes, the status that carries no key, the refusals that
//! never echo one, and the opt-in that gates saving. Every test uses an in-memory backend; the real
//! keychain is never touched.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use orbyn_core::credentials::{Credentials, DEV_SERVICE, Failure, SecretBackend};
use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use zeroize::Zeroizing;

use super::router;
use crate::http::AppState;
use crate::http::access::Access;

const KEY: &str = "sk-live-0123456789abcdef";
const BASE: &str = "/api/settings/engine-credentials";

#[derive(Default)]
struct Memory {
    items: Mutex<HashMap<(String, String), Vec<u8>>>,
    fail: Mutex<Option<Failure>>,
}

impl Memory {
    fn failing(&self) -> Option<Failure> {
        *self.fail.lock().unwrap()
    }
}

impl SecretBackend for Memory {
    fn get(&self, service: &str, account: &str) -> Result<Option<Zeroizing<Vec<u8>>>, Failure> {
        if let Some(f) = self.failing() {
            return Err(f);
        }
        Ok(self
            .items
            .lock()
            .unwrap()
            .get(&(service.into(), account.into()))
            .cloned()
            .map(Zeroizing::new))
    }
    fn set(&self, service: &str, account: &str, bytes: &[u8]) -> Result<(), Failure> {
        if let Some(f) = self.failing() {
            return Err(f);
        }
        self.items
            .lock()
            .unwrap()
            .insert((service.into(), account.into()), bytes.to_vec());
        Ok(())
    }
    fn delete(&self, service: &str, account: &str) -> Result<(), Failure> {
        if let Some(f) = self.failing() {
            return Err(f);
        }
        self.items
            .lock()
            .unwrap()
            .remove(&(service.into(), account.into()));
        Ok(())
    }
}

struct Served {
    port: u16,
    core: Arc<Core>,
    memory: Arc<Memory>,
    dir: std::path::PathBuf,
}

async fn serve(name: &str, remote: bool) -> Served {
    let dir = std::env::temp_dir().join(format!("orbynd-engcred-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let memory = Arc::new(Memory::default());
    let core = Arc::new(
        Core::open(CoreConfig::new(dir.clone(), "http://127.0.0.1:1".into()))
            .unwrap()
            .with_credentials(Credentials::new(memory.clone(), DEV_SERVICE)),
    );
    let app = router(AppState {
        core: core.clone(),
        access: Arc::new(Access::new("token", vec![], None).with_person_key(Some("the-key"))),
        remote,
        ui_dir: None,
        images: Arc::new(orbyn_core::browser::remote_images::ImageProxy::default()),
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    Served {
        port,
        core,
        memory,
        dir,
    }
}

impl Served {
    /// Returns the status, the JSON body and the raw text.
    async fn call(
        &self,
        method: &str,
        path: &str,
        body: Value,
        person: bool,
    ) -> (u16, Value, String) {
        let url = format!("http://127.0.0.1:{}{path}", self.port);
        let mut request = reqwest::Client::new()
            .request(method.parse().unwrap(), url)
            .bearer_auth("token")
            .json(&body);
        if person {
            request = request.header("x-orbyn-person", "the-key");
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (
            status,
            serde_json::from_str(&text).unwrap_or(Value::Null),
            text,
        )
    }

    async fn opt_in(&self, on: bool) {
        let (status, _, _) = self
            .call(
                "PUT",
                &format!("{BASE}/cursor"),
                json!({"opted_in": on}),
                true,
            )
            .await;
        assert_eq!(status, 200);
    }

    async fn status(&self) -> Value {
        let (status, body, _) = self
            .call("GET", &format!("{BASE}/cursor"), json!({}), false)
            .await;
        assert_eq!(status, 200);
        body
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[tokio::test]
async fn the_writes_are_person_routes_and_the_status_is_local() {
    let app = serve("class", false).await;
    for (method, path, body) in [
        ("PUT", format!("{BASE}/cursor"), json!({"opted_in": true})),
        ("PUT", format!("{BASE}/cursor/key"), json!({"key": KEY})),
        ("DELETE", format!("{BASE}/cursor/key"), json!({})),
    ] {
        let (status, reply, text) = app.call(method, &path, body, false).await;
        assert_eq!(
            (status, &reply["code"]),
            (403, &json!("person_required")),
            "{method} {path}"
        );
        assert!(!text.contains(KEY));
    }
    assert_eq!(
        app.status().await["opted_in"],
        json!(false),
        "the service token reads"
    );
    app.opt_in(true).await;
    let (status, _, _) = app
        .call(
            "PUT",
            &format!("{BASE}/cursor/key"),
            json!({"key": KEY}),
            true,
        )
        .await;
    assert_eq!(status, 200, "the desktop key is accepted");
}

#[tokio::test]
async fn a_remote_viewer_cannot_write_but_can_read_the_status() {
    let app = serve("remote", true).await;
    for (method, path) in [
        ("PUT", format!("{BASE}/cursor")),
        ("PUT", format!("{BASE}/cursor/key")),
        ("DELETE", format!("{BASE}/cursor/key")),
    ] {
        let (status, reply, _) = app.call(method, &path, json!({}), true).await;
        assert_eq!(
            (status, &reply["code"]),
            (403, &json!("local_only")),
            "{path}"
        );
    }
    assert_eq!(app.status().await["present"], json!(false));
}

#[tokio::test]
async fn the_status_is_booleans_and_never_the_key() {
    let app = serve("status", false).await;
    assert_eq!(
        app.status().await,
        json!({"engine": "cursor", "opted_in": false, "present": false, "state": "off"})
    );
    app.opt_in(true).await;
    assert_eq!(app.status().await["state"], json!("not_saved"));
    let (status, saved, text) = app
        .call(
            "PUT",
            &format!("{BASE}/cursor/key"),
            json!({"key": KEY}),
            true,
        )
        .await;
    assert_eq!(status, 200);
    assert!(!text.contains("0123456789") && !text.contains("sk-live"));
    assert_eq!(saved, app.status().await);
    let (_, _, text) = app
        .call("GET", &format!("{BASE}/cursor"), json!({}), false)
        .await;
    assert!(!text.contains("0123") && !text.contains("cdef") && !text.contains("sk-"));
    let status = app.status().await;
    let mut names: Vec<&str> = status
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    names.sort_unstable();
    assert_eq!(names, ["engine", "opted_in", "present", "state"]);
    assert_eq!(status["present"], json!(true));
    // The item is in the keychain store only, and the settings table holds a flag, not the key.
    assert_eq!(app.memory.items.lock().unwrap().len(), 1);
    assert_eq!(
        app.core
            .store
            .setting("engine_credentials.cursor")
            .unwrap()
            .as_deref(),
        Some("1")
    );
}

#[tokio::test]
async fn opting_out_blocks_saving_and_reads_off_but_keeps_the_item() {
    let app = serve("optout", false).await;
    let (status, reply, _) = app
        .call(
            "PUT",
            &format!("{BASE}/cursor/key"),
            json!({"key": KEY}),
            true,
        )
        .await;
    assert_eq!((status, &reply["code"]), (409, &json!("opt_in_off")));
    assert!(app.memory.items.lock().unwrap().is_empty());
    app.opt_in(true).await;
    app.call(
        "PUT",
        &format!("{BASE}/cursor/key"),
        json!({"key": KEY}),
        true,
    )
    .await;
    app.opt_in(false).await;
    assert_eq!(app.status().await["state"], json!("off"));
    assert_eq!(
        app.memory.items.lock().unwrap().len(),
        1,
        "off does not delete"
    );
    assert!(app.core.engine_spawn_secret("cursor").unwrap().is_none());
    app.opt_in(true).await;
    assert_eq!(app.status().await["present"], json!(true));
    let secret = app.core.engine_spawn_secret("cursor").unwrap().unwrap();
    assert_eq!((secret.env, secret.expose()), ("CURSOR_API_KEY", KEY));
}

#[tokio::test]
async fn remove_deletes_the_item() {
    let app = serve("remove", false).await;
    app.opt_in(true).await;
    app.call(
        "PUT",
        &format!("{BASE}/cursor/key"),
        json!({"key": KEY}),
        true,
    )
    .await;
    let (status, reply, _) = app
        .call("DELETE", &format!("{BASE}/cursor/key"), json!({}), true)
        .await;
    assert_eq!(status, 200);
    assert_eq!(reply["state"], json!("not_saved"));
    assert_eq!(reply["stopped"], json!(0), "no runs were running");
    assert!(app.memory.items.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_failing_or_refused_save_never_echoes_the_key() {
    let app = serve("echo", false).await;
    app.opt_in(true).await;
    let spaced = "sk-live with a space-9876543210";
    let (status, reply, text) = app
        .call(
            "PUT",
            &format!("{BASE}/cursor/key"),
            json!({"key": spaced}),
            true,
        )
        .await;
    assert_eq!((status, &reply["code"]), (400, &json!("invalid_key")));
    assert!(!text.contains("sk-live") && !text.contains("9876543210"));
    for (failure, code, expected) in [
        (Failure::Unusable, "keychain_unavailable", 503),
        (Failure::Blocked, "keychain_blocked", 409),
        (Failure::Other, "keychain_failed", 503),
    ] {
        *app.memory.fail.lock().unwrap() = Some(failure);
        let (status, reply, text) = app
            .call(
                "PUT",
                &format!("{BASE}/cursor/key"),
                json!({"key": KEY}),
                true,
            )
            .await;
        assert_eq!((status, &reply["code"]), (expected, &json!(code)));
        assert!(
            !text.contains("sk-live") && !text.contains("0123456789"),
            "{text}"
        );
    }
    // A body of the wrong shape names the field, not the value.
    let (status, _, text) = app
        .call(
            "PUT",
            &format!("{BASE}/cursor/key"),
            json!({"key": 123456789}),
            true,
        )
        .await;
    assert_eq!(status, 400);
    assert!(!text.contains("123456789"));
}

#[tokio::test]
async fn an_unknown_engine_is_not_found() {
    let app = serve("unknown", false).await;
    for (method, path) in [
        ("GET", format!("{BASE}/nope")),
        ("PUT", format!("{BASE}/nope")),
        ("PUT", format!("{BASE}/nope/key")),
        ("DELETE", format!("{BASE}/nope/key")),
    ] {
        let (status, reply, _) = app
            .call(method, &path, json!({"opted_in": true, "key": KEY}), true)
            .await;
        assert_eq!(
            (status, &reply["code"]),
            (404, &json!("not_found")),
            "{method} {path}"
        );
    }
    assert!(
        app.core
            .store
            .setting("engine_credentials.nope")
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn retry_is_a_person_route_that_reprobes_and_changes_nothing() {
    let app = serve("retry", false).await;
    app.opt_in(true).await;
    *app.memory.fail.lock().unwrap() = Some(Failure::Blocked);
    let path = format!("{BASE}/cursor/retry");
    let (status, reply, _) = app.call("POST", &path, json!({}), false).await;
    assert_eq!((status, &reply["code"]), (403, &json!("person_required")));
    let (_, blocked, _) = app.call("POST", &path, json!({}), true).await;
    assert_eq!(blocked["state"], json!("blocked"));
    *app.memory.fail.lock().unwrap() = None;
    let (status, ok, _) = app.call("POST", &path, json!({}), true).await;
    assert_eq!((status, &ok["state"]), (200, &json!("not_saved")));
}

#[tokio::test]
async fn a_message_built_from_a_key_shaped_string_is_redacted_on_its_way_out() {
    use axum::Router;
    use axum::middleware::from_fn_with_state;
    use axum::routing::get;

    let app = serve("redact", false).await;
    assert_eq!(
        super::engine_credentials::redact_message(&app.core, &format!("bad {KEY} here")),
        "bad [hidden: API key] here"
    );
    // The layer on a route that fails with such a message.
    let state = AppState {
        core: app.core.clone(),
        access: Arc::new(Access::new("token", vec![], None)),
        remote: false,
        ui_dir: None,
        images: Arc::new(orbyn_core::browser::remote_images::ImageProxy::default()),
    };
    async fn boom() -> Result<&'static str, crate::http::ApiFailure> {
        Err(crate::http::ApiFailure::new(
            axum::http::StatusCode::BAD_REQUEST,
            "invalid_input",
            "refused sk-live-0123456789abcdef today",
        ))
    }
    let router = Router::new()
        .route("/boom", get(boom))
        .route_layer(from_fn_with_state(
            state.clone(),
            super::engine_credentials::redact_errors,
        ))
        .with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, router).await });
    let response = reqwest::get(format!("http://127.0.0.1:{port}/boom"))
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 400);
    let text = response.text().await.unwrap();
    assert!(!text.contains("0123456789abcdef"), "{text}");
    assert!(text.contains("[hidden: API key]"), "{text}");
}

/// Whether `text` holds any run of `len` characters of `key`.
fn shares_a_run(text: &str, key: &str, len: usize) -> bool {
    let chars: Vec<char> = key.chars().collect();
    chars
        .windows(len)
        .any(|run| text.contains(&run.iter().collect::<String>()))
}

#[tokio::test]
async fn no_error_body_holds_any_six_characters_of_the_key() {
    let app = serve("six", false).await;
    let key = "Zq7vK2mXp9RtLw4Y";
    assert!(shares_a_run(key, key, 6), "the helper finds a run");
    let mut bodies = Vec::new();
    // Opt-in off, then every keychain failure, then a key the rules refuse, then a body of the wrong shape.
    let (_, _, text) = app
        .call(
            "PUT",
            &format!("{BASE}/cursor/key"),
            json!({"key": key}),
            true,
        )
        .await;
    bodies.push(text);
    app.opt_in(true).await;
    for failure in [Failure::Unusable, Failure::Blocked, Failure::Other] {
        *app.memory.fail.lock().unwrap() = Some(failure);
        let (_, _, text) = app
            .call(
                "PUT",
                &format!("{BASE}/cursor/key"),
                json!({"key": key}),
                true,
            )
            .await;
        bodies.push(text);
    }
    *app.memory.fail.lock().unwrap() = None;
    let refused = format!("{key} {key}");
    let (_, _, text) = app
        .call(
            "PUT",
            &format!("{BASE}/cursor/key"),
            json!({"key": refused}),
            true,
        )
        .await;
    bodies.push(text);
    let (_, _, text) = app
        .call(
            "PUT",
            &format!("{BASE}/cursor/key"),
            json!({"key": [key]}),
            true,
        )
        .await;
    bodies.push(text);
    let (_, _, text) = app
        .call(
            "PUT",
            &format!("{BASE}/cursor/key"),
            json!({"other": key}),
            true,
        )
        .await;
    bodies.push(text);
    assert_eq!(bodies.len(), 7);
    for body in &bodies {
        assert!(!body.is_empty());
        assert!(!shares_a_run(body, key, 6), "{body}");
    }
}

#[tokio::test]
async fn a_key_in_the_url_is_never_taken() {
    let app = serve("url", false).await;
    app.opt_in(true).await;
    let key = "Zq7vK2mXp9RtLw4Y";
    let client = reqwest::Client::new();
    for url in [
        format!("http://127.0.0.1:{}{BASE}/cursor/key?key={key}", app.port),
        format!("http://127.0.0.1:{}{BASE}/cursor/{key}", app.port),
    ] {
        let response = client
            .put(url)
            .bearer_auth("token")
            .header("x-orbyn-person", "the-key")
            .json(&json!({}))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_client_error());
        assert!(!shares_a_run(&response.text().await.unwrap(), key, 6));
    }
    assert!(app.memory.items.lock().unwrap().is_empty());
    assert_eq!(app.status().await["state"], json!("not_saved"));
}

#[tokio::test]
async fn the_engine_hosts_secret_source_is_the_cores_and_follows_the_opt_in() {
    use orbyn_core::agents::driver::host::{CoreCell, secrets_from};

    let app = serve("hostsource", false).await;
    let cell = CoreCell::default();
    let cursor = secrets_from(cell.clone(), "cursor");
    // Not bound to a core yet: no key, never a fallback to the keychain.
    assert!(cursor().is_none());
    cell.set(Arc::downgrade(&app.core)).unwrap();
    app.opt_in(true).await;
    app.call(
        "PUT",
        &format!("{BASE}/cursor/key"),
        json!({"key": KEY}),
        true,
    )
    .await;
    let secret = cursor().expect("opted in with a saved key");
    assert_eq!((secret.env, secret.expose()), ("CURSOR_API_KEY", KEY));
    // A family the allowlist does not name gets nothing, whatever is saved.
    assert!(secrets_from(cell.clone(), "claude")().is_none());
    // Turning it off stops the next spawn from getting the key.
    app.opt_in(false).await;
    assert!(cursor().is_none());
}
