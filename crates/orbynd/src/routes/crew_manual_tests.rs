//! `POST /api/crew/tasks/{id}/checks/{check}/decide` over HTTP: the person layer is the only way in, the Preview flag hides it, and the
//! service token alone is not enough. The rules and the storage are `orbyn-core`'s (`crew::person_check` and its tests).

use std::path::PathBuf;
use std::sync::Arc;

use orbyn_core::crew::changes::changes_crew_state;
use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::router;
use crate::http::AppState;
use crate::http::access::Access;

const DECIDE: &str = "/api/crew/tasks/sha256:0/checks/chk-manual/decide";

struct Served {
    port: u16,
    _core: Arc<Core>,
    dir: PathBuf,
}

async fn serve(name: &str, preview: bool, access: Access) -> Served {
    let dir = std::env::temp_dir().join(format!("orbynd-manual-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = CoreConfig::new(dir.clone(), "http://127.0.0.1:1".into());
    config.crew_preview = preview;
    config.crew_confine_workers = false;
    let core = Arc::new(Core::open(config).unwrap());
    let app = router(AppState {
        core: core.clone(),
        access: Arc::new(access),
        remote: false,
        ui_dir: None,
        images: Arc::new(orbyn_core::browser::remote_images::ImageProxy::default()),
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    Served {
        port,
        _core: core,
        dir,
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// One request with the service token, and the person key in the header the desktop app sends, when given.
impl Served {
    async fn post(&self, path: &str, token: bool, person: Option<&str>) -> (u16, Value) {
        let mut request = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}{path}", self.port))
            .json(&json!({ "decision": "pass" }));
        if token {
            request = request.bearer_auth("token");
        }
        if let Some(key) = person {
            request = request.header("x-orbyn-person", key);
        }
        let answer = request.send().await.unwrap();
        let status = answer.status().as_u16();
        (status, answer.json().await.unwrap_or(Value::Null))
    }
}

/// The route changes a task's state, so Autopilot's N1 refuses an agent's request to it by path.
#[test]
fn the_decide_route_is_listed_as_a_state_changing_crew_route() {
    assert!(changes_crew_state(DECIDE));
}

/// Only this launch's person key reaches a decision: the service token a worker can read is not enough, and nothing is decided.
#[tokio::test]
async fn a_decision_needs_the_person_key_and_the_token_alone_is_refused() {
    let app = serve(
        "key",
        true,
        Access::new("token", vec![], None).with_person_key(Some("the-key")),
    )
    .await;
    // The service token, and no person key at all.
    let (status, reply) = app.post(DECIDE, true, None).await;
    assert_eq!((status, &reply["code"]), (403, &json!("person_required")));
    // The service token, and the wrong person key.
    let (status, reply) = app.post(DECIDE, true, Some("wrong")).await;
    assert_eq!((status, &reply["code"]), (403, &json!("person_required")));
    // No token at all: the credential comes first.
    let (status, _) = app.post(DECIDE, false, Some("the-key")).await;
    assert_eq!(status, 401);
}

/// With the flag off the route is not there, whatever the caller carries.
#[tokio::test]
async fn the_route_is_hidden_while_the_preview_flag_is_off() {
    let app = serve(
        "off",
        false,
        Access::new("token", vec![], None).with_dev_person_override(true),
    )
    .await;
    let (status, reply) = app.post(DECIDE, true, None).await;
    assert_eq!((status, &reply["code"]), (404, &json!("not_found")));
}
