//! `GET /api/audit` over HTTP: a stored row's `detail` is masked when it is read, in its values and
//! in its keys, and the fields a screen reads stay readable. The row itself is never rewritten. A
//! redaction setting that does not read fails the whole read instead of showing the rows unscrubbed.

use std::path::PathBuf;
use std::sync::Arc;

use orbyn_contracts::{PatternKind, PatternScope, RedactionPatternInput};
use orbyn_core::store::AuditRecord;
use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::router;
use crate::http::AppState;
use crate::http::access::Access;

struct Served {
    port: u16,
    core: Arc<Core>,
    dir: PathBuf,
}

async fn serve(name: &str) -> Served {
    let dir = std::env::temp_dir().join(format!("orbynd-audit-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = CoreConfig::new(dir.join("data"), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    let core = Arc::new(Core::open(config).unwrap());
    let app = router(AppState {
        core: core.clone(),
        access: Arc::new(Access::new("token", vec![], None).with_dev_person_override(true)),
        remote: false,
        ui_dir: None,
        images: Arc::new(orbyn_core::browser::remote_images::ImageProxy::default()),
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    Served { port, core, dir }
}

impl Served {
    /// A row as a permission ask writes it: engine-supplied JSON in `detail`.
    fn ask(&self, detail: Value) {
        self.core
            .store
            .append_audit(&AuditRecord {
                actor: "local".into(),
                action: "permission.ask".into(),
                target: "cargo test".into(),
                decision: None,
                source: "unknown".into(),
                thread_id: None,
                run_id: None,
                detail,
            })
            .unwrap();
    }

    /// The permission row's stored `detail`, as written (a later redaction row may be newest).
    fn ask_detail(&self) -> String {
        self.core
            .store
            .list_audit(None, 50)
            .unwrap()
            .into_iter()
            .find(|event| event.action == "permission.ask")
            .unwrap()
            .detail
            .to_string()
    }

    fn pattern(&self, name: &str, text: &str) {
        self.core
            .save_redaction_pattern(
                "local",
                None,
                &RedactionPatternInput {
                    name: name.into(),
                    pattern: text.into(),
                    kind: PatternKind::Text,
                    applies_to: PatternScope::Anywhere,
                    accept_broad: None,
                },
            )
            .unwrap();
    }

    async fn audit(&self) -> (u16, String) {
        let response = reqwest::Client::new()
            .get(format!("http://127.0.0.1:{}/api/audit?limit=50", self.port))
            .bearer_auth("token")
            .send()
            .await
            .unwrap();
        (response.status().as_u16(), response.text().await.unwrap())
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[tokio::test]
async fn a_secret_in_a_stored_detail_is_masked_on_read_in_a_value_and_in_a_key() {
    let app = serve("mask").await;
    // A synthetic canary, never a real secret: a value and a key the person asked to hide.
    let canary = format!("{}-{}", "zebra", "canary");
    let mut nested = serde_json::Map::new();
    nested.insert(canary.clone(), json!(["a", 1, true, null]));
    app.ask(json!({
        "note": format!("the {canary} field was read"),
        "nested": Value::Object(nested),
    }));
    // Stored as written.
    let stored = app.ask_detail();
    assert!(stored.contains(&canary));
    app.pattern("Canary", &canary);

    let (status, text) = app.audit().await;
    assert_eq!(status, 200, "{text}");
    assert!(
        !text.contains(&canary),
        "a detail value or key escaped the person's patterns: {text}"
    );
    assert!(text.contains("[hidden: Canary]"), "not masked by name");
    // The fields a screen reads, and the ordinary keys around the canary, stay readable.
    for field in [
        "\"action\":\"permission.ask\"",
        "cargo test",
        "note",
        "nested",
    ] {
        assert!(text.contains(field), "the field {field} was over-masked");
    }
    // The read never rewrites what is stored.
    assert_eq!(app.ask_detail(), stored);
}

#[tokio::test]
async fn a_redaction_setting_that_does_not_read_fails_the_read_instead_of_showing_the_rows() {
    let app = serve("unreadable").await;
    app.ask(json!({ "note": "nothing a screen should see without the patterns" }));
    app.core
        .store
        .set_setting("redaction", "{ not json")
        .unwrap();
    let (status, text) = app.audit().await;
    assert_eq!(status, 500, "{text}");
    assert!(text.contains("internal"), "{text}");
    assert!(!text.contains("nothing a screen"), "{text}");
}
