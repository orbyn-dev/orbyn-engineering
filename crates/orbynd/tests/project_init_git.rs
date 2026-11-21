//! F16: a project folder added without Git can become a repository over HTTP.
//!
//! The route runs Git in a folder the person trusted, adds only `.git`, and makes no
//! commit. An untrusted folder, a folder that is already a repository and a folder that
//! is gone are refused with their own codes.

mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::{Options, Service, TempDir};

async fn start(dir: &TempDir) -> Service {
    let service = Service::start(&dir.path().join("data"), Options::default());
    service.ensure_workspace(dir.path()).await;
    service
}

/// Every file under `root` with its bytes, `.git` included.
fn tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(folder).unwrap().flatten() {
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                folders.push(path);
            } else {
                files.insert(path.clone(), std::fs::read(&path).unwrap_or_default());
            }
        }
    }
    files
}

fn plain_folder(dir: &TempDir, name: &str) -> PathBuf {
    let folder = dir.path().join(name);
    std::fs::create_dir_all(folder.join("src")).unwrap();
    std::fs::write(folder.join("a.txt"), "one\n").unwrap();
    std::fs::write(folder.join("src/b.txt"), "two\n").unwrap();
    folder
}

async fn added(service: &Service, path: &Path) -> Value {
    let (code, project) = service
        .send("/api/projects/add", json!({ "path": path }))
        .await;
    assert_eq!(code, 201, "{project}");
    project
}

async fn trust(service: &Service, id: &str) {
    let (code, body) = service
        .send(&format!("/api/projects/{id}/trust"), json!({}))
        .await;
    assert_eq!(code, 200, "{body}");
}

async fn init(service: &Service, id: &str) -> (u16, Value) {
    service
        .send(&format!("/api/projects/{id}/git/init"), json!({}))
        .await
}

fn git_output(folder: &Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new("git")
        .args(args)
        .current_dir(folder)
        .output()
        .unwrap()
}

#[tokio::test]
async fn init_git_over_http_turns_a_trusted_folder_into_a_repository() {
    let dir = TempDir::new();
    let service = start(&dir).await;
    let folder = plain_folder(&dir, "plain");
    let project = added(&service, &folder).await;
    let id = project["id"].as_str().unwrap();
    assert_eq!(project["is_git"], false);

    let (code, body) = init(&service, id).await;
    assert_eq!(
        (code, body["code"].as_str()),
        (403, Some("folder_untrusted")),
        "{body}"
    );
    assert!(!folder.join(".git").exists());

    trust(&service, id).await;
    let before = tree(&folder);
    let (code, project) = init(&service, id).await;
    assert_eq!(code, 200, "{project}");
    assert_eq!(project["is_git"], true);
    let after = tree(&folder);
    for (path, bytes) in &before {
        assert_eq!(after.get(path), Some(bytes), "{} changed", path.display());
    }
    let added: Vec<&PathBuf> = after
        .keys()
        .filter(|path| !before.contains_key(*path))
        .collect();
    assert!(!added.is_empty());
    assert!(
        added
            .iter()
            .all(|path| path.strip_prefix(&folder).unwrap().starts_with(".git")),
        "only .git is new: {added:?}"
    );
    let branch = git_output(&folder, &["branch", "--show-current"]);
    assert_eq!(String::from_utf8_lossy(&branch.stdout).trim(), "main");
    assert!(
        !git_output(&folder, &["rev-parse", "--verify", "HEAD"])
            .status
            .success(),
        "no commit is invented"
    );

    let (code, body) = init(&service, id).await;
    assert_eq!(
        (code, body["code"].as_str()),
        (400, Some("invalid_input")),
        "{body}"
    );
    assert_eq!(body["message"], "This project is already a Git repository.");
    service.stop().await;
}

#[tokio::test]
async fn init_git_over_http_refuses_a_folder_that_is_gone() {
    let dir = TempDir::new();
    let service = start(&dir).await;
    let folder = plain_folder(&dir, "plain");
    let project = added(&service, &folder).await;
    let id = project["id"].as_str().unwrap();
    trust(&service, id).await;
    std::fs::remove_dir_all(&folder).unwrap();
    let (code, body) = init(&service, id).await;
    assert_eq!(
        (code, body["code"].as_str()),
        (400, Some("invalid_input")),
        "{body}"
    );
    assert_eq!(body["message"], "The project's folder is gone.");
    let projects = service.json("/api/projects").await;
    assert_eq!(
        projects
            .as_array()
            .unwrap()
            .iter()
            .find(|project| project["id"] == id)
            .unwrap()["is_git"],
        false
    );
    service.stop().await;
}
