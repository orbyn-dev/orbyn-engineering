//! The repository lookup and the starred-repository list through the real
//! service: registered checkouts with real Git remotes, and a stand-in `gh` binary
//! that answers the same `/user/starred` read the person's own CLI answers.

mod support;

use std::path::PathBuf;

use serde_json::{Value, json};
use support::{Options, Service, TempDir, run_git};

/// A real Git repository whose origin is `remote`, so the app reads its identity
/// from the local Git config exactly as it does for a project the person added.
fn repository(dir: &TempDir, name: &str, remote: &str) -> PathBuf {
    let path = dir.path().join("repos").join(name);
    std::fs::create_dir_all(&path).unwrap();
    run_git(&path, &["init", "-q", "-b", "main"]);
    run_git(&path, &["config", "user.email", "t@example.com"]);
    run_git(&path, &["config", "user.name", "T"]);
    std::fs::write(path.join("README.md"), "fixture\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-q", "-m", "first"]);
    run_git(&path, &["remote", "add", "origin", remote]);
    path
}

async fn get(service: &Service, path: &str) -> (u16, Value) {
    let response = service.get(path).send().await.unwrap();
    let status = response.status().as_u16();
    let text = response.text().await.unwrap();
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

/// The lookup route's query, with the repository's own slash kept encoded.
fn lookup(repository: &str) -> String {
    format!(
        "/api/source-control/repository?repository={}",
        repository.replace('/', "%2F")
    )
}

/// Starts a service whose `gh` is the fake engine binary, printing `stars` for the
/// starred-list read. `fail:<message>` refuses like a signed-out CLI and `hang`
/// never answers.
fn star_service(dir: &TempDir, stars: &str) -> (Service, PathBuf) {
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::copy(
        env!("CARGO_BIN_EXE_orbyn-fake-engine"),
        bin.join(format!("gh{}", std::env::consts::EXE_SUFFIX)),
    )
    .unwrap();
    let fixture = dir.path().join("gh-stars");
    std::fs::write(&fixture, stars).unwrap();
    let arguments = dir.path().join("gh-arguments");
    let path = std::env::join_paths(
        std::iter::once(bin).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let service = Service::start(
        &dir.path().join("data"),
        Options {
            env: vec![
                ("PATH".into(), path.to_string_lossy().into_owned()),
                (
                    "ORBYN_FAKE_GH_STARS".into(),
                    fixture.to_string_lossy().into_owned(),
                ),
                (
                    "ORBYN_FAKE_GH_ARGUMENTS".into(),
                    arguments.to_string_lossy().into_owned(),
                ),
            ],
            ..Options::default()
        },
    );
    (service, arguments)
}

fn starred_row(owner: &str, name: &str, stars: u32) -> Value {
    json!({
        "id": 1,
        "full_name": format!("{owner}/{name}"),
        "html_url": format!("https://github.com/{owner}/{name}"),
        "description": "A fixture",
        "private": false,
        "archived": false,
        "default_branch": "main",
        "language": "Rust",
        "stargazers_count": stars,
    })
}

#[tokio::test]
async fn a_repository_is_found_by_the_owner_name_its_checkouts_remote_names() {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), Options::default());
    let wanted = repository(
        &dir,
        "wanted",
        "https://github.com/Orbyn-Dev/Orbyn-Engineering.git",
    );
    let other = repository(&dir, "other", "https://gitlab.com/other/thing.git");
    let wanted_id = service.project(&wanted, true).await;
    let other_id = service.project(&other, true).await;

    let (status, body) = get(&service, &lookup("orbyn-dev/orbyn-engineering")).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["complete"], json!(true), "{body}");
    let checkouts = body["checkouts"].as_array().unwrap();
    assert_eq!(checkouts.len(), 1, "{body}");
    assert_eq!(checkouts[0]["project_id"], json!(wanted_id), "{body}");
    assert_eq!(
        checkouts[0]["repository_label"],
        json!("Orbyn-Dev/Orbyn-Engineering"),
        "{body}"
    );
    assert_eq!(checkouts[0]["name"], json!("wanted"), "{body}");
    assert!(
        checkouts[0]["repository_key"]
            .as_str()
            .is_some_and(|key| !key.is_empty()),
        "{body}"
    );
    assert!(
        checkouts
            .iter()
            .all(|checkout| checkout["project_id"] != json!(other_id))
    );

    // The remote keeps its path case; matching the person's own spelling ignores it.
    let (status, again) = get(&service, &lookup("ORBYN-DEV/ORBYN-ENGINEERING")).await;
    assert_eq!(status, 200, "{again}");
    assert_eq!(again["checkouts"].as_array().unwrap().len(), 1, "{again}");

    let (status, missing) = get(&service, &lookup("nobody/nothing")).await;
    assert_eq!(status, 200, "{missing}");
    assert_eq!(missing["checkouts"], json!([]), "{missing}");
    assert_eq!(missing["complete"], json!(true), "{missing}");
    service.stop().await;
}

#[tokio::test]
async fn a_repository_name_that_is_not_owner_name_is_refused() {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), Options::default());
    for bad in [
        "",
        "owner",
        "owner/",
        "/name",
        "owner/../name",
        "owner/na%20me",
        "owner//name",
    ] {
        let (status, body) = get(&service, &lookup(bad)).await;
        assert_eq!(status, 400, "{bad}: {body}");
        assert_eq!(body["code"], json!("invalid_input"), "{bad}: {body}");
    }
    service.stop().await;
}

#[tokio::test]
async fn a_checkout_that_is_gone_leaves_the_lookup_incomplete() {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), Options::default());
    let wanted = repository(&dir, "wanted", "https://github.com/o/r.git");
    service.project(&wanted, true).await;
    std::fs::remove_dir_all(&wanted).unwrap();

    let (status, body) = get(&service, &lookup("o/r")).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["checkouts"], json!([]), "{body}");
    assert_eq!(body["complete"], json!(false), "{body}");
    service.stop().await;
}

#[tokio::test]
async fn starred_repositories_are_the_users_own_list_through_the_gh_cli() {
    let dir = TempDir::new();
    let stars = json!([
        starred_row("orbyn-dev", "orbyn-engineering", 7),
        starred_row("someone", "else", 1),
    ]);
    let (service, arguments) = star_service(&dir, &stars.to_string());

    let (status, body) = get(&service, "/api/source-control/starred").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["truncated"], json!(false), "{body}");
    assert!(body.get("unavailable").is_none_or(Value::is_null), "{body}");
    assert_eq!(
        body["repositories"],
        json!([
            {
                "owner": "orbyn-dev",
                "name": "orbyn-engineering",
                "full_name": "orbyn-dev/orbyn-engineering",
                "url": "https://github.com/orbyn-dev/orbyn-engineering",
                "description": "A fixture",
                "private": false,
                "archived": false,
                "default_branch": "main",
                "language": "Rust",
                "stars": 7,
            },
            {
                "owner": "someone",
                "name": "else",
                "full_name": "someone/else",
                "url": "https://github.com/someone/else",
                "description": "A fixture",
                "private": false,
                "archived": false,
                "default_branch": "main",
                "language": "Rust",
                "stars": 1,
            },
        ]),
        "{body}"
    );

    // The read is one bounded page of the same resource the fixed star check reads.
    let call: Value = serde_json::from_slice(&std::fs::read(&arguments).unwrap()).unwrap();
    assert_eq!(
        call["args"],
        json!([
            "api",
            "user/starred?per_page=100",
            "--hostname",
            "github.com"
        ])
    );
    service.stop().await;
}

#[tokio::test]
async fn a_full_starred_page_reports_that_the_list_is_truncated() {
    let dir = TempDir::new();
    let rows: Vec<Value> = (0..100)
        .map(|index| starred_row("someone", &format!("repo-{index}"), index))
        .collect();
    let (service, _) = star_service(&dir, &Value::Array(rows).to_string());

    let (status, body) = get(&service, "/api/source-control/starred").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["truncated"], json!(true), "{body}");
    assert_eq!(
        body["repositories"].as_array().unwrap().len(),
        100,
        "{body}"
    );
    service.stop().await;
}

#[tokio::test]
async fn a_starred_read_that_fails_says_why_instead_of_listing_nothing() {
    let dir = TempDir::new();
    let (service, _) = star_service(&dir, "fail:HTTP 401: Bad credentials");
    let (status, body) = get(&service, "/api/source-control/starred").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["repositories"], json!([]), "{body}");
    assert_eq!(body["truncated"], json!(false), "{body}");
    assert_eq!(
        body["unavailable"],
        json!("HTTP 401: Bad credentials"),
        "{body}"
    );

    // A CLI that prints something other than a list is not an empty starred list.
    std::fs::write(dir.path().join("gh-stars"), "Checking authorization\n").unwrap();
    let (status, body) = get(&service, "/api/source-control/starred").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["repositories"], json!([]), "{body}");
    assert!(
        body["unavailable"]
            .as_str()
            .is_some_and(|reason| reason.contains("starred list")),
        "{body}"
    );
    service.stop().await;
}

#[tokio::test]
async fn a_hanging_starred_read_is_bounded() {
    let dir = TempDir::new();
    let (service, _) = star_service(&dir, "hang");
    let started = std::time::Instant::now();
    let (status, body) = get(&service, "/api/source-control/starred").await;
    assert_eq!(status, 200, "{body}");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(25),
        "the read was not bounded: {:?}",
        started.elapsed()
    );
    assert!(
        body["unavailable"]
            .as_str()
            .is_some_and(|reason| reason.contains("did not answer in time")),
        "{body}"
    );
    service.stop().await;
}
