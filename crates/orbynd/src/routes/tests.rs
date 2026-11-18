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
//! Each protected row also carries its class, `local` or `person` (`classes.rs`). A route
//! with no class fails `every_route_has_a_class`. Nothing is enforced by class yet.
//!
//! After an intended route change, regenerate the file with
//! `ORBYN_UPDATE_ROUTES=1 cargo test -p orbynd --bin orbynd route_table`, then give each new route a class.

use std::collections::BTreeSet;
use std::sync::Arc;

use serde_json::{Value, json};

use axum::Router;
use orbyn_core::{Core, CoreConfig};
use tokio::net::TcpListener;

use super::classes::{ENFORCED, FIELD_LEVEL, PERSON};
use super::router;
use crate::http::AppState;
use crate::http::access::Access;

const TABLE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/routes.json");
const PUBLIC_PREFIXES: [&str; 3] = ["/auth/", "/hooks/", "/mcp/"];
const METHODS: [&str; 9] = [
    "get", "head", "delete", "options", "patch", "post", "put", "trace", "connect",
];

/// Every `(method, path)` the router registers, read from its `Debug` text.
pub(super) fn registered(router: &Router) -> BTreeSet<(String, String)> {
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
    // The Crew routes hide behind the Preview flag; with it on they are protected routes like the rest.
    let mut config = CoreConfig::new(dir.clone(), "http://127.0.0.1:1".into());
    let absent = dir.join("absent-native");
    assert!(absent.is_absolute() && !absent.exists());
    config.engines.claude_program = absent.join("claude.exe").to_string_lossy().into_owned();
    config.engines.codex_program = absent.join("codex.exe").to_string_lossy().into_owned();
    config.engines.known_engine_ids = Some(Vec::new());
    config.engines.lookup_latest = false;
    config.crew_preview = true;
    config.crew_confine_workers = false;
    let core = Arc::new(Core::open(config).unwrap());
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

/// The pinned table, read from `tests/routes.json`.
fn pinned() -> Value {
    serde_json::from_str(&std::fs::read_to_string(TABLE_PATH).unwrap()).unwrap()
}

/// The `(method, path)` of each row, the class dropped.
fn without_class(table: &Value) -> Value {
    let rows: Vec<Value> = table["protected"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| json!([row[0], row[1]]))
        .collect();
    json!({ "protected": rows, "public": table["public"] })
}

/// Adds each route's class: `person` from the table in `classes.rs`, else the class already
/// pinned, else `unclassified`, which `every_route_has_a_class` refuses.
fn with_classes(now: &Value, old: Option<&Value>) -> Value {
    let rows: Vec<Value> = now["protected"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let (method, path) = (row[0].as_str().unwrap(), row[1].as_str().unwrap());
            let class = if PERSON.iter().any(|(m, p, _)| (*m, *p) == (method, path)) {
                json!("person")
            } else {
                old.and_then(|old| old["protected"].as_array())
                    .and_then(|rows| rows.iter().find(|r| r[0] == row[0] && r[1] == row[1]))
                    .map_or(json!("unclassified"), |r| r[2].clone())
            };
            json!([method, path, class])
        })
        .collect();
    json!({ "protected": rows, "public": now["public"] })
}

#[tokio::test]
async fn route_table_is_unchanged() {
    let now = observed().await;
    if std::env::var_os("ORBYN_UPDATE_ROUTES").is_some() {
        let old = std::fs::read_to_string(TABLE_PATH)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok());
        std::fs::write(
            TABLE_PATH,
            one_line_per_route(&with_classes(&now, old.as_ref())),
        )
        .unwrap();
        return;
    }
    assert_eq!(
        now,
        without_class(&pinned()),
        "the route table changed; if that is intended, regenerate tests/routes.json and classify each new route"
    );
}

/// A route nobody classified (a new one is written `unclassified` by the regenerate) fails here, and the
/// `person` rows are exactly the reviewed table in `classes.rs`.
#[test]
fn every_route_has_a_class() {
    let table = pinned();
    let rows = table["protected"].as_array().unwrap();
    let unclassified: Vec<String> = rows
        .iter()
        .filter(|row| !matches!(row[2].as_str(), Some("local" | "person")))
        .map(|row| format!("{} {}", row[0], row[1]))
        .collect();
    assert!(
        unclassified.is_empty(),
        "classify these routes as local or person in tests/routes.json (and list a person route in routes/classes.rs): {unclassified:?}"
    );
    let person: BTreeSet<(String, String)> = rows
        .iter()
        .filter(|row| row[2] == "person")
        .map(|row| {
            (
                row[0].as_str().unwrap().to_owned(),
                row[1].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let listed: BTreeSet<(String, String)> = PERSON
        .iter()
        .map(|(m, p, _)| ((*m).to_owned(), (*p).to_owned()))
        .collect();
    assert_eq!(listed.len(), PERSON.len(), "a Person route is listed twice");
    assert!(PERSON.iter().all(|(_, _, why)| !why.is_empty()));
    assert_eq!(
        person, listed,
        "the person rows of tests/routes.json and routes/classes.rs PERSON must be the same routes"
    );
}

/// Enforcement is a one-line allowlist, and each line must name a `Person` route that exists: a typo, or a route that was
/// renamed or reclassified `local`, would otherwise leave a route unguarded without a failure.
#[test]
fn every_enforced_route_is_a_person_route_in_the_table() {
    let table = pinned();
    let rows = table["protected"].as_array().unwrap();
    assert!(!ENFORCED.is_empty());
    let mut seen = BTreeSet::new();
    for (method, path) in ENFORCED {
        assert!(
            seen.insert((method, path)),
            "{method} {path} is listed twice"
        );
        assert!(
            PERSON.iter().any(|(m, p, _)| (m, p) == (method, path)),
            "{method} {path} is enforced but is not in PERSON"
        );
        assert!(
            rows.iter()
                .any(|row| row[0] == *method && row[1] == *path && row[2] == "person"),
            "{method} {path} is enforced but is not a person row of tests/routes.json"
        );
    }
}

/// Step 3 enforces every `Person` route, so a new one cannot be classified without the key. A route that must stay
/// unenforced for a while would be listed here with its reason, and the doc's "Still open" says so.
#[test]
fn every_person_route_is_enforced() {
    let missing: Vec<String> = PERSON
        .iter()
        .filter(|(m, p, _)| !ENFORCED.iter().any(|(em, ep)| (em, ep) == (m, p)))
        .map(|(m, p, _)| format!("{m} {p}"))
        .collect();
    assert!(
        missing.is_empty(),
        "these Person routes do not need the person key: {missing:?}"
    );
}

/// A field-level route is `Local` in the table (it needs no key for most requests) and its handler checks the key.
#[test]
fn every_field_level_route_is_a_local_row_in_the_table() {
    let table = pinned();
    let rows = table["protected"].as_array().unwrap();
    assert!(!FIELD_LEVEL.is_empty());
    for (method, path, field) in FIELD_LEVEL {
        assert!(!field.is_empty());
        assert!(
            !ENFORCED.iter().any(|(m, p)| (m, p) == (method, path)),
            "{method} {path} is enforced as a whole, so it is not field-level"
        );
        assert!(
            rows.iter()
                .any(|row| row[0] == *method && row[1] == *path && row[2] == "local"),
            "{method} {path} is field-level but is not a local row of tests/routes.json"
        );
    }
}
