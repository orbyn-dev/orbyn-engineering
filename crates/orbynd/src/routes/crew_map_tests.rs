//! The Map, message and fact routes over HTTP: empty views for an empty crew, bad input refused, open to a remote viewer (they only
//! read), and not there with the flag off. What each view holds is checked by the core tests (`crew::tests::flow_map`); the table,
//! credential and flag-off walks of `crew_tests.rs` cover these routes too, since they sit under `/api/crew`.

use serde_json::json;

use super::crew_tasks_tests::serve;

const PATHS: [&str; 3] = [
    "/api/crew/flow/map?crew=nobody",
    "/api/crew/flow/messages?crew=nobody",
    "/api/crew/flow/facts?crew=nobody",
];

#[tokio::test]
async fn an_empty_crew_has_empty_views_and_bad_input_is_refused() {
    let app = serve("map-empty", true, false).await;
    let (status, map) = app.get(PATHS[0]).await;
    assert_eq!(
        (status, map["nodes"].clone(), map["truncated"].clone()),
        (200, json!([]), json!(false))
    );
    let (status, messages) = app.get(PATHS[1]).await;
    assert_eq!((status, messages["total"].clone()), (200, json!(0)));
    let (status, facts) = app.get(PATHS[2]).await;
    assert_eq!((status, facts["facts"].clone()), (200, json!([])));
    for path in [
        "/api/crew/flow/map",
        "/api/crew/flow/map?crew=..%2Fetc",
        "/api/crew/flow/messages",
        "/api/crew/flow/messages?crew=crew-1&member=..%2Fx",
        "/api/crew/flow/messages?crew=crew-1&limit=0",
        "/api/crew/flow/messages?crew=crew-1&limit=201",
        "/api/crew/flow/messages?crew=crew-1&offset=abc",
        "/api/crew/flow/facts",
        "/api/crew/flow/facts?crew=a%20b",
    ] {
        let (status, body) = app.get(path).await;
        assert_eq!(status, 400, "{path}");
        assert_eq!(body["code"], "invalid_input", "{path}");
    }
}

#[tokio::test]
async fn a_remote_viewer_may_read_them_and_the_flag_off_hides_them() {
    let remote = serve("map-remote", true, true).await;
    for path in PATHS {
        let (status, _) = remote.get(path).await;
        assert_eq!(status, 200, "{path}");
    }
    let off = serve("map-off", false, false).await;
    let (_, unknown) = off.get("/api/crew-nothing").await;
    for path in PATHS {
        let (status, body) = off.get(path).await;
        assert_eq!(status, 404, "{path}");
        assert_eq!(body, unknown, "{path} answers as an absent route does");
    }
}

/// A GET never forces a probe of every engine: the route and the map read the registry's cached statuses.
#[test]
fn the_map_route_never_asks_the_registry_to_refresh() {
    let forced = ["statuses(", "true)"].concat();
    assert!(!include_str!("crew_map.rs").contains(&forced));
}
