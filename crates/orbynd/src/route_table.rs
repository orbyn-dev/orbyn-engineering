//! Characterization test for the HTTP route table. It builds the real router and
//! compares every `(method, path)` and its protection with `tests/routes.json`, so
//! a moved, dropped, duplicated or newly exposed route fails here.
//!
//! axum has no public route listing, so the routes are read from the `Debug` text
//! of `Router`. Protection is observed, not read from the code: the router is served
//! on a loopback port and every route gets one request with no credential. A
//! protected route answers 401 from `require_auth`; the routes that are public on
//! purpose (`/auth`, `/hooks`, `/mcp`, and the UI fallback) record their status.
//!
//! Limit: `note_activity` sits inside `require_auth` and cannot be seen without a
//! credential, so it is pinned only by both layers being applied to the same
//! merged router in `router`.
//!
//! After an intended route change, regenerate the file with
//! `ORBYN_UPDATE_ROUTES=1 cargo test -p orbynd --bin orbynd route_table`.

use std::collections::BTreeSet;
use std::sync::Arc;

use serde_json::{Value, json};

use super::*;

const TABLE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/routes.json");
const PUBLIC_PREFIXES: [&str; 3] = ["/auth/", "/hooks/", "/mcp/"];
const METHODS: [&str; 9] = [
    "get", "head", "delete", "options", "patch", "post", "put", "trace", "connect",
];

/// Every `(method, path)` the router registers, read from its `Debug` text.
fn registered(router: &Router) -> BTreeSet<(String, String)> {
    let text = format!("{router:?}");
    let (routes, node) = text
        .split_once("node: Node { paths: {")
        .expect("the router lists its paths");
    let node = node.split_once("} }").expect("the path list ends").0;
    let mut paths = std::collections::BTreeMap::new();
    for entry in node.split("RouteId(").skip(1) {
        let (id, rest) = entry.split_once("): \"").expect("a path entry");
        let path = rest.split_once('"').expect("a closing quote").0;
        paths.insert(id.parse::<usize>().unwrap(), path.to_owned());
    }
    let mut found = BTreeSet::new();
    let mut seen = 0;
    for entry in routes.split("RouteId(").skip(1) {
        let Some((id, rest)) = entry.split_once("): MethodRouter(MethodRouter { ") else {
            continue;
        };
        seen += 1;
        let path = &paths[&id.parse::<usize>().unwrap()];
        let fields = rest.split_once(", fallback").expect("method fields").0;
        for field in fields.split(", ") {
            let (name, value) = field.split_once(": ").expect("a method field");
            assert!(METHODS.contains(&name), "unexpected field {name}");
            if value != "None" {
                found.insert((name.to_ascii_uppercase(), path.clone()));
            }
        }
    }
    assert_eq!(seen, paths.len(), "every path has one method router");
    found
}

/// The table as it is today: protected routes, and public ones with their status.
async fn observed() -> Value {
    let dir = std::env::temp_dir().join(format!("orbynd-routes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let core =
        Arc::new(Core::open(CoreConfig::new(dir.clone(), "http://127.0.0.1:1".into())).unwrap());
    let access = Arc::new(Access::new("token", vec![], None));
    let app = router(AppState {
        core,
        access,
        remote: false,
        ui_dir: None,
        images: Arc::new(orbyn_core::browser::remote_images::ImageProxy::default()),
    });
    let listed = registered(&app);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    let client = reqwest::Client::new();
    let probe = |method: &str, path: &str| {
        let url = format!("http://127.0.0.1:{port}{}", path.replace("{pid}", "1"));
        let url = url.replace(['{', '}'], "");
        let request = client.request(method.parse().unwrap(), url);
        async move { request.send().await.unwrap().status().as_u16() }
    };
    let (mut protected, mut public) = (Vec::new(), Vec::new());
    for (method, path) in &listed {
        let status = probe(method, path).await;
        if PUBLIC_PREFIXES
            .iter()
            .any(|prefix| path.starts_with(prefix))
        {
            public.push(json!([method, path, status]));
        } else {
            assert_eq!(status, 401, "{method} {path} must need a credential");
            protected.push(json!([method, path]));
        }
    }
    // Nothing else matched: the UI fallback answers, with no credential too.
    for (method, path) in [
        ("GET", "/not-a-file"),
        ("GET", "/api/not-a-route"),
        ("POST", "/not-a-file"),
    ] {
        public.push(json!([
            method,
            format!("fallback {path}"),
            probe(method, path).await
        ]));
    }
    let _ = std::fs::remove_dir_all(dir);
    json!({ "protected": protected, "public": public })
}

/// JSON with one route per line, so a changed route is one line in a diff.
fn one_line_per_route(table: &Value) -> String {
    let section = |name: &str| {
        let rows: Vec<String> = table[name]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| format!("    {row}"))
            .collect();
        format!("  \"{name}\": [\n{}\n  ]", rows.join(",\n"))
    };
    format!("{{\n{},\n{}\n}}\n", section("protected"), section("public"))
}

#[tokio::test]
async fn route_table_is_unchanged() {
    let now = observed().await;
    if std::env::var_os("ORBYN_UPDATE_ROUTES").is_some() {
        std::fs::write(TABLE_PATH, one_line_per_route(&now)).unwrap();
        return;
    }
    let pinned: Value =
        serde_json::from_str(&std::fs::read_to_string(TABLE_PATH).unwrap()).unwrap();
    assert_eq!(
        now, pinned,
        "the route table changed; if that is intended, regenerate tests/routes.json"
    );
}
