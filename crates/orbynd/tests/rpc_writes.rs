//! R1/R5 write-shaped RPC methods a Replica needs: a thread's general file
//! `mutate` (create, rename, delete) and the explicit VCS `refreshStatus`,
//! `createRef`, `switchRef` and `removeWorktree` handlers.

mod support;

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
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

struct Fixture {
    dir: TempDir,
    service: Service,
    thread: String,
    /// The thread's checkout: the project folder for a thread on the current checkout.
    checkout: PathBuf,
}

async fn git_fixture() -> Fixture {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), Options::default());
    let folder = dir.path().join("project");
    repo(&folder);
    let project = service.project(&folder, true).await;
    let thread = service.thread(&project, "codex").await;
    Fixture {
        dir,
        service,
        thread,
        checkout: folder,
    }
}

/// A project whose folder is outside Git, added in place and trusted. `POST
/// /api/projects/create` would initialise Git, which is exactly what this test needs to avoid.
async fn non_git_project(service: &Service, dir: &TempDir, name: &str) -> String {
    let folder = dir.path().join(name);
    std::fs::create_dir_all(&folder).unwrap();
    let (status, project) = service
        .send("/api/projects/add", json!({ "path": folder }))
        .await;
    assert_eq!(status, 201, "{project}");
    let id = project["id"].as_str().unwrap().to_owned();
    let (status, _) = service
        .send(&format!("/api/projects/{id}/trust"), json!({}))
        .await;
    assert_eq!(status, 200);
    id
}

/// A worktree thread, made through the same route the app uses.
async fn worktree_thread(service: &Service, project: &str, purpose: &str) -> (String, PathBuf) {
    let (status, thread) = service
        .send(
            "/api/threads",
            json!({
                "project_id": project, "engine": "codex", "model": null, "title": null,
                "worktree": purpose, "skill_profile": null,
            }),
        )
        .await;
    assert_eq!(status, 201, "{thread}");
    let path = PathBuf::from(thread["cwd"].as_str().unwrap());
    (thread["id"].as_str().unwrap().to_owned(), path)
}

/// The opaque id the explorer lists for the worktree at `path`.
async fn worktree_id(service: &Service, project: &str, name: &str) -> String {
    let listed = service
        .json(&format!("/api/projects/{project}/worktrees?refresh=1"))
        .await;
    listed["worktrees"]
        .as_array()
        .unwrap()
        .iter()
        .find(|worktree| worktree["name"] == name)
        .unwrap_or_else(|| panic!("{name} is listed: {listed}"))["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn delete(service: &Service, path: &str) -> (u16, Value) {
    let response = service
        .client
        .delete(format!("{}{path}", service.url))
        .bearer_auth(&service.token)
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    (status, response.json().await.unwrap_or(Value::Null))
}

// ---------------------------------------------------------------- R1 mutate

#[tokio::test]
async fn a_threads_file_mutate_creates_renames_and_deletes() {
    let f = git_fixture().await;
    let route = format!("/api/threads/{}/files/mutate", f.thread);
    for (action, expected) in [
        (json!({"action": "create_directory", "path": "new"}), 204),
        (json!({"action": "create_file", "path": "new/empty"}), 204),
        (
            json!({"action": "move", "path": "new", "destination": "renamed"}),
            204,
        ),
        (
            json!({"action": "move", "path": "renamed/empty", "destination": "empty"}),
            204,
        ),
        (json!({"action": "delete", "path": "renamed"}), 204),
        (json!({"action": "delete", "path": "empty"}), 204),
    ] {
        let response = f.service.post(&route).json(&action).send().await.unwrap();
        assert_eq!(
            response.status().as_u16(),
            expected,
            "{action}: {}",
            response.text().await.unwrap()
        );
    }
    assert!(!f.checkout.join("new").exists());
    assert!(!f.checkout.join("renamed").exists());
    assert!(f.checkout.join("a.txt").exists());
    // The work is recorded as a person's file action, not as an engine tool.
    let audit = f.service.json("/api/audit?limit=200").await;
    assert!(
        audit
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["action"] == "file.mutate")
    );
}

#[tokio::test]
async fn file_mutate_refuses_paths_outside_the_checkout_and_git_metadata() {
    let f = git_fixture().await;
    let route = format!("/api/threads/{}/files/mutate", f.thread);
    for action in [
        json!({"action": "create_file", "path": "../escape"}),
        json!({"action": "create_file", "path": ".git/config"}),
        json!({"action": "create_file", "path": "src/.Git/hooks/pre-commit"}),
        json!({"action": "create_file", "path": "/etc/passwd"}),
        json!({"action": "delete", "path": ""}),
        json!({"action": "delete", "path": ".git"}),
        json!({"action": "move", "path": "a.txt", "destination": "../stolen"}),
        json!({"action": "move", "path": "src", "destination": "src/inner"}),
        json!({"action": "create_file", "path": "a.txt"}),
        json!({"action": "delete", "path": "src"}),
        json!({"action": "create_file", "path": "x", "payload": "ignored"}),
    ] {
        let response = f.service.post(&route).json(&action).send().await.unwrap();
        assert_eq!(
            response.status().as_u16(),
            400,
            "{action}: {}",
            response.text().await.unwrap()
        );
    }
    assert_eq!(
        std::fs::read_to_string(f.checkout.join("a.txt")).unwrap(),
        "one\n"
    );
    assert!(f.checkout.join("src").exists());
    assert!(!f.dir.path().join("escape").exists());
    assert!(!f.dir.path().join("stolen").exists());
}

// ------------------------------------------------ person class for both writes

#[tokio::test]
async fn file_mutate_and_worktree_removal_need_the_person_key() {
    let dir = TempDir::new();
    let folder = dir.path().join("project");
    repo(&folder);
    // A desktop shell holds the person key: the token alone must not write files or remove a
    // worktree, while the keyed page can.
    let service = Service::start(
        &dir.path().join("data"),
        Options {
            args: vec!["--parent-stdin".into()],
            env: vec![("ORBYN_DEV_DISABLE_PERSON_KEY".into(), "0".into())],
        },
    );
    let project = service.project(&folder, true).await;
    let thread = service.thread(&project, "codex").await;

    let mutate = format!("/api/threads/{thread}/files/mutate");
    let action = json!({"action": "create_file", "path": "new.txt"});
    let refused = service
        .client
        .post(format!("{}{mutate}", service.url))
        .bearer_auth(&service.token)
        .json(&action)
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status().as_u16(), 403);
    let body: Value = refused.json().await.unwrap();
    assert_eq!(body["code"], "person_required", "{body}");
    assert!(!folder.join("new.txt").exists());
    let (status, body) = service.send(&mutate, action).await;
    assert_eq!(status, 204, "{body}");
    assert!(folder.join("new.txt").exists());

    let (worktree_thread, worktree_path) = worktree_thread(&service, &project, "keyed").await;
    let path = format!(
        "/api/projects/{project}/worktrees/{}",
        worktree_id(&service, &project, "keyed").await
    );
    let refused = service
        .client
        .delete(format!("{}{path}", service.url))
        .bearer_auth(&service.token)
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status().as_u16(), 403);
    let body: Value = refused.json().await.unwrap();
    assert_eq!(body["code"], "person_required", "{body}");
    assert!(worktree_path.exists());
    assert!(!worktree_thread.is_empty());
}

// ---------------------------------------------------------- R5 refreshStatus

#[tokio::test]
async fn vcs_refresh_status_reads_git_fresh_and_refuses_a_non_git_folder() {
    let f = git_fixture().await;
    let route = format!("/api/threads/{}/vcs/refresh-status", f.thread);
    let (status, body) = f.service.send(&route, json!({})).await;
    assert_eq!(status, 200, "{body}");
    let head = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(&f.checkout)
        .output()
        .unwrap();
    assert_eq!(body["branch"], "main");
    assert_eq!(body["detached"], false);
    assert_eq!(body["head"], String::from_utf8(head.stdout).unwrap().trim());
    assert!(body["upstream"].is_null());
    assert_eq!(body["ahead"], 0);
    assert_eq!(body["behind"], 0);
    assert_eq!(body["changed"], 0);
    assert_eq!(body["untracked"], 0);
    assert_eq!(body["conflicted"], 0);

    // The answer is read now, not kept from a previous ask.
    std::fs::write(f.checkout.join("a.txt"), "two\n").unwrap();
    std::fs::write(f.checkout.join("loose.txt"), "loose\n").unwrap();
    let (status, body) = f.service.send(&route, json!({})).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["changed"], 1);
    assert_eq!(body["untracked"], 1);

    // A folder outside Git has no VCS status to refresh.
    let project = non_git_project(&f.service, &f.dir, "plain").await;
    let thread = f.service.thread(&project, "codex").await;
    let (status, body) = f
        .service
        .send(
            &format!("/api/threads/{thread}/vcs/refresh-status"),
            json!({}),
        )
        .await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["code"], "invalid_input", "{body}");

    // An untrusted checkout is not read at all.
    let untrusted = f.dir.path().join("untrusted");
    repo(&untrusted);
    let project = f.service.project(&untrusted, false).await;
    let thread = f.service.thread(&project, "codex").await;
    let (status, body) = f
        .service
        .send(
            &format!("/api/threads/{thread}/vcs/refresh-status"),
            json!({}),
        )
        .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["code"], "folder_untrusted", "{body}");
}

// ----------------------------------------------------- R5 createRef/switchRef

#[tokio::test]
async fn vcs_refs_create_without_switching_switch_and_refuse_bad_or_non_git() {
    let f = git_fixture().await;
    let refs = format!("/api/threads/{}/vcs/refs", f.thread);
    let (status, body) = f
        .service
        .send(&refs, json!({ "name": "topic", "start": "main" }))
        .await;
    assert_eq!(status, 201, "{body}");
    // Creating a ref leaves the checkout where it was.
    let branches = f
        .service
        .json(&format!("/api/threads/{}/branches", f.thread))
        .await;
    let branch = |name: &str| {
        branches
            .as_array()
            .unwrap()
            .iter()
            .find(|branch| branch["name"] == name)
            .cloned()
    };
    assert_eq!(branch("topic").unwrap()["current"], false);
    assert_eq!(branch("main").unwrap()["current"], true);

    let (status, body) = f
        .service
        .send(&format!("{refs}/switch"), json!({ "name": "topic" }))
        .await;
    assert_eq!(status, 204, "{body}");
    let branches = f
        .service
        .json(&format!("/api/threads/{}/branches", f.thread))
        .await;
    let current: Vec<_> = branches
        .as_array()
        .unwrap()
        .iter()
        .filter(|branch| branch["current"] == true)
        .collect();
    assert_eq!(current[0]["name"], "topic");

    for name in ["--force", "a..b", ""] {
        let (status, body) = f.service.send(&refs, json!({ "name": name })).await;
        assert_eq!(status, 400, "create {name}: {body}");
        let (status, body) = f
            .service
            .send(&format!("{refs}/switch"), json!({ "name": name }))
            .await;
        assert_eq!(status, 400, "switch {name}: {body}");
    }
    let (status, body) = f
        .service
        .send(&format!("{refs}/switch"), json!({ "name": "missing" }))
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["code"], "git_failed", "{body}");

    // A folder outside Git has no refs to create or switch.
    let project = non_git_project(&f.service, &f.dir, "plain").await;
    let thread = f.service.thread(&project, "codex").await;
    for (path, body) in [
        (
            format!("/api/threads/{thread}/vcs/refs"),
            json!({ "name": "topic" }),
        ),
        (
            format!("/api/threads/{thread}/vcs/refs/switch"),
            json!({ "name": "topic" }),
        ),
    ] {
        let (status, body) = f.service.send(&path, body).await;
        assert_eq!(status, 400, "{path}: {body}");
        assert_eq!(body["code"], "invalid_input", "{path}: {body}");
    }
}

// ------------------------------------------------------- R5 removeWorktree

#[tokio::test]
async fn worktree_removal_deletes_only_a_worktree_orbyn_made_and_nobody_uses() {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), Options::default());
    let folder = dir.path().join("project");
    repo(&folder);
    let project = service.project(&folder, true).await;

    let (thread, path) = worktree_thread(&service, &project, "topic").await;
    let base = format!(
        "/api/projects/{project}/worktrees/{}",
        worktree_id(&service, &project, "topic").await
    );

    // A thread still works here: the folder is its checkout, so it stays.
    let (status, body) = delete(&service, &base).await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["code"], "policy_denied", "{body}");
    assert!(path.exists());

    // Once the thread is gone the worktree is Orbyn's leftover, and it is removed with Git.
    let (status, _) = delete(&service, &format!("/api/threads/{thread}")).await;
    assert_eq!(status, 204);
    let (status, body) = delete(&service, &base).await;
    assert_eq!(status, 204, "{body}");
    assert!(!path.exists());
    let listed = std::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(&folder)
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&listed.stdout).contains("project-wt/topic"));

    // The primary checkout is never a worktree to remove.
    let main = service
        .json(&format!("/api/projects/{project}/worktrees?refresh=1"))
        .await;
    let main_id = main["worktrees"]
        .as_array()
        .unwrap()
        .iter()
        .find(|worktree| worktree["is_main"] == true)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (status, body) = delete(
        &service,
        &format!("/api/projects/{project}/worktrees/{main_id}"),
    )
    .await;
    assert_eq!(status, 400, "{body}");
    assert!(folder.join("a.txt").exists());

    // A worktree someone else made outside Orbyn's sibling root is not the app's.
    let manual = dir.path().join("manual");
    run_git(
        &folder,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "manual",
            manual.to_str().unwrap(),
        ],
    );
    let manual_id = worktree_id(&service, &project, "manual").await;
    let (status, body) = delete(
        &service,
        &format!("/api/projects/{project}/worktrees/{manual_id}"),
    )
    .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["code"], "policy_denied", "{body}");
    assert!(manual.exists());

    // An id that names no member of the project is not found.
    let (status, _) = delete(
        &service,
        &format!("/api/projects/{project}/worktrees/wt-0000000000000000"),
    )
    .await;
    assert_eq!(status, 404);
    let (status, _) = delete(&service, "/api/projects/none/worktrees/wt-0000000000000000").await;
    assert_eq!(status, 404);
}
