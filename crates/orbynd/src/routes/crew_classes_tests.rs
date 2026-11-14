//! The data-class routes over HTTP: they are reads, they sit behind the Preview flag, and they refuse what is not a thread, a
//! task or a path. The table test in `crew_tests.rs` checks that all three are listed as reads for the marker Autopilot refuses by,
//! and the core tests (`crew::tests::classes`) check what each view holds.

use std::sync::Arc;

use orbyn_core::crew::changes::changes_crew_state;
use orbyn_core::{Core, CoreConfig};
use serde_json::Value;
use tokio::net::TcpListener;

use super::router;
use crate::http::AppState;
use crate::http::access::Access;

async fn serve(name: &str, preview: bool) -> (u16, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("orbynd-classes-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = CoreConfig::new(dir.join("data"), "http://127.0.0.1:1".into());
    config.crew_preview = preview;
    let core = Arc::new(Core::open(config).unwrap());
    let app = router(AppState {
        core,
        access: Arc::new(Access::new("token", vec![], None)),
        remote: false,
        ui_dir: None,
        images: Arc::new(orbyn_core::browser::remote_images::ImageProxy::default()),
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    (port, dir)
}

async fn get(port: u16, path: &str) -> (u16, Value) {
    let response = reqwest::Client::new()
        .get(format!("http://127.0.0.1:{port}{path}"))
        .bearer_auth("token")
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    let text = response.text().await.unwrap();
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

#[tokio::test]
async fn the_class_routes_are_hidden_with_the_flag_off() {
    let (port, dir) = serve("off", false).await;
    let (_, absent) = get(port, "/api/crew-nothing").await;
    for path in [
        "/api/crew/classes?thread=t1",
        "/api/crew/classes/test?thread=t1&path=src/a.rs",
        "/api/crew/route/t1",
    ] {
        let (status, body) = get(port, path).await;
        assert_eq!(status, 404, "{path}");
        assert_eq!(body, absent, "{path} answers as an absent route does");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn the_class_routes_refuse_what_names_nothing_and_change_nothing() {
    let (port, dir) = serve("on", true).await;
    // No thread named, and a thread that does not exist.
    assert_eq!(get(port, "/api/crew/classes").await.0, 400);
    assert_eq!(get(port, "/api/crew/classes?thread=nope").await.0, 404);
    assert_eq!(get(port, "/api/crew/classes/test?path=a").await.0, 400);
    assert_eq!(
        get(port, "/api/crew/classes/test?thread=nope&path=a")
            .await
            .0,
        404
    );
    // A malformed query answers in the app's error shape, not plain text.
    let (status, body) = get(port, "/api/crew/classes/test?thread=t&path=%FF%FE&x=%").await;
    assert!(status == 400 || status == 404, "{status}");
    assert!(body.get("code").is_some() || status == 404, "{body}");
    // A task that does not exist, and an id that is not one.
    assert_eq!(get(port, "/api/crew/route/nope").await.0, 404);
    assert_eq!(get(port, "/api/crew/route/a%20b").await.0, 400);
    // All three are reads for the marker Autopilot refuses by: an agent may look and cannot be mistaken for a change.
    for path in [
        "/api/crew/classes",
        "/api/crew/classes/test",
        "/api/crew/route/t1",
    ] {
        assert!(!changes_crew_state(path), "{path}");
    }
    // There is no write route here: any other method is refused.
    for method in ["POST", "PUT", "PATCH", "DELETE"] {
        let status = reqwest::Client::new()
            .request(
                method.parse().unwrap(),
                format!("http://127.0.0.1:{port}/api/crew/classes"),
            )
            .bearer_auth("token")
            .send()
            .await
            .unwrap()
            .status()
            .as_u16();
        assert_eq!(status, 405, "{method}");
    }
    let _ = std::fs::remove_dir_all(dir);
}
