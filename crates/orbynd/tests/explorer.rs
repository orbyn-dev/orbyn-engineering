//! The explorer's read-only routes through the real service.

mod support;

use std::path::Path;

use serde_json::Value;
use support::{Options, Service, TempDir, run_git};

fn repo(path: &Path) {
    std::fs::create_dir_all(path.join("src")).unwrap();
    run_git(path, &["init", "-q", "-b", "main"]);
    run_git(path, &["config", "user.email", "t@example.com"]);
    run_git(path, &["config", "user.name", "T"]);
    std::fs::write(path.join("a.txt"), "one\n").unwrap();
    std::fs::write(path.join("src/lib.rs"), "pub fn x() {}\n").unwrap();
    run_git(path, &["add", "."]);
    run_git(path, &["commit", "-q", "-m", "first"]);
}

async fn get(service: &Service, path: &str) -> (u16, Value) {
    let response = service.get(path).send().await.unwrap();
    let status = response.status().as_u16();
    (status, response.json().await.unwrap_or(Value::Null))
}

struct Fixture {
    _dir: TempDir,
    service: Service,
    project: String,
    worktree: String,
}

async fn fixture() -> Fixture {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), Options::default());
    let folder = dir.path().join("app");
    repo(&folder);
    let project = service.project(&folder, true).await;
    let listed = service
        .json(&format!("/api/projects/{project}/worktrees"))
        .await;
    let worktree = listed["worktrees"][0]["id"].as_str().unwrap().to_owned();
    Fixture {
        _dir: dir,
        service,
        project,
        worktree,
    }
}

fn base(f: &Fixture) -> String {
    format!("/api/projects/{}/worktrees/{}", f.project, f.worktree)
}

#[tokio::test]
async fn worktrees_tree_and_file_answer_with_the_documented_shapes() {
    let f = fixture().await;
    let listed = f
        .service
        .json(&format!("/api/projects/{}/worktrees", f.project))
        .await;
    assert_eq!(listed["project_id"], f.project.as_str());
    assert_eq!(listed["git"], true);
    let main = &listed["worktrees"][0];
    assert_eq!(main["is_main"], true);
    assert_eq!(main["branch"], "main");
    assert_eq!(main["read_only"], false);
    assert!(main["threads"].as_array().unwrap().is_empty());

    std::fs::write(
        Path::new(main["path"].as_str().unwrap()).join("a.txt"),
        "two\n",
    )
    .unwrap();
    let tree = f
        .service
        .json(&format!("{}/tree?refresh=1", base(&f)))
        .await;
    assert_eq!(tree["changes"], "ready");
    assert_eq!(tree["truncated"], false);
    let names: Vec<&str> = tree["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["src", "a.txt"]);
    assert_eq!(tree["entries"][1]["change"], "modified");
    assert_eq!(tree["entries"][0]["kind"], "directory");
    assert_eq!(tree["entries"][0]["size"], Value::Null);

    let src = f
        .service
        .json(&format!("{}/tree?path=src&filter=LIB", base(&f)))
        .await;
    assert_eq!(src["entries"][0]["path"], "src/lib.rs");

    let file = f
        .service
        .json(&format!("{}/file?path=a.txt", base(&f)))
        .await;
    assert_eq!(file["kind"], "text");
    assert_eq!(file["content"], "two\n");
    assert_eq!(file["change"], "modified");
    assert_eq!(file["read_only"], false);
}

#[tokio::test]
async fn encoded_and_odd_paths_never_leave_the_worktree() {
    let f = fixture().await;
    for query in [
        "path=%2e%2e",
        "path=%2E%2E%2Fdata",
        "path=src%2F%2e%2e%2F%2e%2e",
        "path=..%5Cdata",
        "path=src%5Clib.rs",
        "path=%2Fetc",
        "path=%2Fetc%2Fhosts",
        "path=.git",
        "path=%2egit%2Fconfig",
        "path=.GIT",
        "path=a%00b",
    ] {
        for route in ["tree", "file"] {
            let (status, body) = get(&f.service, &format!("{}/{route}?{query}", base(&f))).await;
            assert_eq!(status, 400, "{route} {query}: {body}");
            assert_eq!(body["code"], "invalid_input");
        }
    }
    // The first decode is the service's; a second one never happens.
    let (status, _) = get(&f.service, &format!("{}/file?path=%252e%252e", base(&f))).await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn ids_are_checked_and_one_project_cannot_reach_anothers_worktree() {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), Options::default());
    let (first, second) = (dir.path().join("first"), dir.path().join("second"));
    repo(&first);
    repo(&second);
    let first_id = service.project(&first, true).await;
    let second_id = service.project(&second, true).await;
    let foreign = service
        .json(&format!("/api/projects/{second_id}/worktrees"))
        .await["worktrees"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    for route in ["tree", "file?path=a.txt"] {
        let url = |id: &str| format!("/api/projects/{first_id}/worktrees/{id}/{route}");
        for id in [foreign.as_str(), "wt-0000000000000000", "..", "main"] {
            let (status, body) = get(&service, &url(id)).await;
            assert_eq!(status, 404, "{route} {id}: {body}");
        }
    }
    let (status, _) = get(
        &service,
        &format!("/api/projects/{second_id}/worktrees/{foreign}/tree"),
    )
    .await;
    assert_eq!(status, 200);
    let (status, _) = get(&service, "/api/projects/none/worktrees").await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn an_untrusted_project_is_refused() {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), Options::default());
    let folder = dir.path().join("app");
    repo(&folder);
    let project = service.project(&folder, false).await;
    let (status, body) = get(&service, &format!("/api/projects/{project}/worktrees")).await;
    assert_eq!(status, 403, "{body}");
}

#[tokio::test]
async fn the_routes_need_a_credential_and_offer_no_write() {
    let f = fixture().await;
    let routes = [
        format!("/api/projects/{}/worktrees", f.project),
        format!("{}/tree", base(&f)),
        format!("{}/file?path=a.txt", base(&f)),
    ];
    for route in &routes {
        let bare = f
            .service
            .client
            .get(format!("{}{route}", f.service.url))
            .send()
            .await
            .unwrap();
        assert_eq!(bare.status(), 401, "{route}");
        // A credential in the URL opens only streams and raw files, not these.
        let sneaky = f
            .service
            .client
            .get(format!(
                "{}{route}{}token={}",
                f.service.url,
                if route.contains('?') { '&' } else { '?' },
                f.service.token
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(sneaky.status(), 401, "{route}");
        for method in [
            reqwest::Method::POST,
            reqwest::Method::PUT,
            reqwest::Method::PATCH,
            reqwest::Method::DELETE,
        ] {
            let write = f
                .service
                .client
                .request(method.clone(), format!("{}{route}", f.service.url))
                .bearer_auth(&f.service.token)
                .send()
                .await
                .unwrap();
            assert_eq!(write.status(), 405, "{method} {route}");
        }
    }
}
