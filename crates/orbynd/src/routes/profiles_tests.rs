//! Execution profiles over HTTP: the five writes are `Person` routes a service token alone cannot use, a grant-raising profile is
//! never applied without the person's acknowledgement, and the list is a bare array with the built-ins first.

use std::sync::Arc;

use orbyn_core::store::{NewProject, NewThread};
use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::router;
use crate::http::AppState;
use crate::http::access::Access;

const KEY: &str = "the-key";

struct Served {
    port: u16,
    core: Arc<Core>,
    thread: String,
    dir: std::path::PathBuf,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

async fn serve(name: &str) -> Served {
    let dir = std::env::temp_dir().join(format!("orbynd-profiles-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = CoreConfig::new(dir.join("data"), "http://127.0.0.1:1".into());
    config.crew_preview = true;
    let core = Arc::new(Core::open(config).unwrap());
    let project = core
        .store
        .add_project(NewProject {
            path: &dir.to_string_lossy(),
            name: "p",
            is_git: false,
            managed: false,
            scratch: false,
        })
        .unwrap();
    let thread = core
        .store
        .create_thread(NewThread {
            project_id: project.id,
            title: "t".into(),
            engine: "claude".into(),
            model: None,
            cwd: dir.to_string_lossy().into_owned(),
            worktree: None,
            skill_profile: None,
            parent_thread_id: None,
            relation: None,
            scratch: false,
        })
        .unwrap();
    let app = router(AppState {
        core: core.clone(),
        access: Arc::new(Access::new("token", vec![], None).with_person_key(Some(KEY))),
        remote: false,
        ui_dir: None,
        images: Arc::new(orbyn_core::browser::remote_images::ImageProxy::default()),
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    Served {
        port,
        core,
        thread: thread.id,
        dir,
    }
}

impl Served {
    /// The service token alone, or with the person key the desktop shell holds.
    async fn call(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        person: bool,
    ) -> (u16, Value) {
        let mut request = reqwest::Client::new()
            .request(
                method.parse().unwrap(),
                format!("http://127.0.0.1:{}{path}", self.port),
            )
            .bearer_auth("token");
        if person {
            request = request.header("x-orbyn-person", KEY);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    fn mode(&self) -> String {
        let mode = self
            .core
            .store
            .thread(&self.thread)
            .unwrap()
            .lifecycle
            .unwrap()
            .permission_mode;
        serde_json::to_value(mode)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn applied_rows(&self) -> usize {
        self.core
            .store
            .list_audit(None, 500)
            .unwrap()
            .iter()
            .filter(|row| row.action == "profile.applied")
            .count()
    }
}

fn draft(name: &str, mode: &str, autopilot: &str) -> Value {
    json!({ "name": name, "description": "", "engine": null, "model_tier": "builder", "permission_mode": mode, "autopilot_mode": autopilot })
}

#[tokio::test]
async fn a_service_token_alone_is_refused_on_every_write_and_nothing_changes() {
    let s = serve("token").await;
    let (status, made) = s
        .call(
            "POST",
            "/api/profiles",
            Some(draft("Mine", "supervised", "off")),
            true,
        )
        .await;
    assert_eq!(status, 201, "{made}");
    let id = made["id"].as_str().unwrap().to_owned();
    let writes: [(&str, String, Value); 5] = [
        (
            "POST",
            "/api/profiles".into(),
            draft("Other", "supervised", "off"),
        ),
        (
            "PATCH",
            format!("/api/profiles/{id}"),
            json!({ "seen_revision": 1, "name": "Hijacked" }),
        ),
        ("DELETE", format!("/api/profiles/{id}"), Value::Null),
        (
            "PUT",
            format!("/api/threads/{}/profile", s.thread),
            json!({ "profile_id": id }),
        ),
        (
            "POST",
            "/api/crew/profile/apply".into(),
            json!({ "crew_id": "c1", "profile_id": id }),
        ),
    ];
    for (method, path, body) in writes {
        let body = (!body.is_null()).then_some(body);
        let (status, answer) = s.call(method, &path, body, false).await;
        assert_eq!(
            (status, answer["code"].as_str()),
            (403, Some("person_required")),
            "{method} {path}"
        );
    }
    // Nothing was created, renamed, deleted or applied.
    let (_, list) = s.call("GET", "/api/profiles", None, false).await;
    let names: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Careful", "Fast", "Cheap", "Mine"]);
    assert_eq!(s.applied_rows(), 0);
    assert_eq!(s.mode(), "supervised");
}

#[tokio::test]
async fn the_list_is_a_bare_array_with_the_built_ins_first_and_reads_need_no_key() {
    let s = serve("list").await;
    let (status, list) = s.call("GET", "/api/profiles", None, false).await;
    assert_eq!(status, 200);
    let list = list.as_array().expect("a bare array, no wrapper");
    let ids: Vec<&str> = list.iter().map(|p| p["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["builtin:careful", "builtin:fast", "builtin:cheap"]);
    assert!(
        list.iter()
            .all(|p| p["builtin"] == true && p["availability"]["engine"] == "none")
    );
    assert!(
        list.iter()
            .all(|p| p["permission_mode"] != "full_access" && p["autopilot_mode"] != "full")
    );
    let (status, one) = s
        .call("GET", "/api/profiles/builtin:fast", None, false)
        .await;
    assert_eq!(
        (status, one["name"].clone(), one["warnings"].clone()),
        (200, json!("Fast"), json!([]))
    );
    let (status, missing) = s.call("GET", "/api/profiles/nope", None, false).await;
    assert_eq!((status, missing["code"].as_str()), (404, Some("not_found")));
    // The check saves nothing and is a read.
    let (status, check) = s
        .call(
            "POST",
            "/api/profiles/check",
            Some(draft("Draft", "auto", "on")),
            false,
        )
        .await;
    assert_eq!((status, check["warnings"].clone()), (200, json!([])));
    let (_, after) = s.call("GET", "/api/profiles", None, false).await;
    assert_eq!(after.as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn built_ins_cannot_be_edited_or_deleted_and_read_only_does_not_exist() {
    let s = serve("builtin").await;
    let (status, body) = s
        .call(
            "PATCH",
            "/api/profiles/builtin:careful",
            Some(json!({ "seen_revision": 1, "name": "x" })),
            true,
        )
        .await;
    assert_eq!(
        (status, body["code"].as_str()),
        (403, Some("builtin_profile"))
    );
    let (status, body) = s
        .call("DELETE", "/api/profiles/builtin:cheap", None, true)
        .await;
    assert_eq!(
        (status, body["code"].as_str()),
        (403, Some("builtin_profile"))
    );
    let (status, body) = s
        .call(
            "POST",
            "/api/profiles",
            Some(draft("Mine", "read_only", "off")),
            true,
        )
        .await;
    assert_eq!(
        (status, body["code"].as_str()),
        (400, Some("invalid_input"))
    );
    assert!(
        !body["message"].as_str().unwrap().contains("read_only"),
        "the value is never echoed"
    );
    let (status, body) = s
        .call(
            "POST",
            "/api/profiles",
            Some(draft("careful", "auto", "off")),
            true,
        )
        .await;
    assert_eq!((status, body["code"].as_str()), (409, Some("name_taken")));
}

#[tokio::test]
async fn the_fifty_first_profile_is_refused() {
    let s = serve("limit").await;
    for n in 0..50 {
        let (status, body) = s
            .call(
                "POST",
                "/api/profiles",
                Some(draft(&format!("p{n}"), "supervised", "off")),
                true,
            )
            .await;
        assert_eq!(status, 201, "{body}");
    }
    let (status, body) = s
        .call(
            "POST",
            "/api/profiles",
            Some(draft("extra", "supervised", "off")),
            true,
        )
        .await;
    assert_eq!(
        (status, body["code"].as_str()),
        (409, Some("profile_limit"))
    );
}

#[tokio::test]
async fn a_full_access_or_autopilot_full_profile_applies_nothing_without_the_acknowledgement() {
    let s = serve("confirm").await;
    let (_, full) = s
        .call(
            "POST",
            "/api/profiles",
            Some(draft("Full", "full_access", "full")),
            true,
        )
        .await;
    let id = full["id"].as_str().unwrap();
    let path = format!("/api/threads/{}/profile", s.thread);
    for ack in [json!([]), json!(["full_access"]), json!(["autopilot_full"])] {
        let (status, body) = s
            .call(
                "PUT",
                &path,
                Some(json!({ "profile_id": id, "ack_grants": ack })),
                true,
            )
            .await;
        assert_eq!(
            (status, body["code"].as_str()),
            (409, Some("confirm_required")),
            "{ack}"
        );
        let listed = body["detail"]["grants"].as_array().unwrap();
        assert!(
            !listed.is_empty()
                && listed
                    .iter()
                    .all(|g| g == "full_access" || g == "autopilot_full")
        );
        assert!(
            listed.iter().all(|g| !ack.as_array().unwrap().contains(g)),
            "only what the acknowledgement did not cover"
        );
    }
    assert_eq!(s.mode(), "supervised");
    assert_eq!(s.applied_rows(), 0);
    // The crew route asks the same, before it looks for the crew.
    let (status, body) = s
        .call(
            "POST",
            "/api/crew/profile/apply",
            Some(json!({ "crew_id": "c1", "profile_id": id })),
            true,
        )
        .await;
    assert_eq!(
        (status, body["code"].as_str()),
        (409, Some("confirm_required"))
    );
    // A built-in never asks: it goes on to the machine's checks (its engine is the thread's), not to a confirm.
    let (_, body) = s
        .call(
            "PUT",
            &path,
            Some(json!({ "profile_id": "builtin:careful" })),
            true,
        )
        .await;
    assert_ne!(body["code"].as_str(), Some("confirm_required"));
}
