//! Timeline data and file operations through the real service.

mod support;

use serde_json::{Value, json};
use support::{Options, Service, TempDir, run_git, setup};

async fn write(service: &Service, thread: &str, body: Value) -> (u16, Value) {
    let response = service
        .client
        .put(format!(
            "{}/api/threads/{thread}/files/content",
            service.url
        ))
        .bearer_auth(&service.token)
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    (status, response.json().await.unwrap_or(Value::Null))
}

async fn read(service: &Service, thread: &str, path: &str) -> (u16, Value) {
    let response = service
        .get(&format!("/api/threads/{thread}/files/content"))
        .query(&[("path", path)])
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    (status, response.json().await.unwrap_or(Value::Null))
}

async fn metadata(service: &Service, thread: &str, path: &str) -> (u16, Value) {
    let response = service
        .get(&format!("/api/threads/{thread}/files/metadata"))
        .query(&[("path", path)])
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    (status, response.json().await.unwrap_or(Value::Null))
}

#[tokio::test]
async fn items_keep_raw_engine_data_and_usage_is_reported() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    service.admit(&thread, "[approve]").await;
    let approval = service.pending().await;
    service.decide(approval["id"].as_str().unwrap(), true).await;
    service.finished(&thread).await;
    let item = service
        .json(&format!("/api/threads/{thread}/items/cmd1"))
        .await;
    assert_eq!(item["kind"], "commandExecution");
    assert_eq!(item["exit_code"], 0);
    assert_eq!(item["raw"]["aggregatedOutput"], "listing");
    let usage = service.json(&format!("/api/threads/{thread}/usage")).await;
    assert_eq!(usage["total_tokens"], 120);
    assert_eq!(usage["context_window"], 1000);
    let events = service.all_events(&thread).await;
    assert!(
        events
            .iter()
            .any(|event| event["event"]["name"] == "tool.output")
    );
    assert!(
        events
            .iter()
            .any(|event| event["event"]["name"] == "tool.finished")
    );
    assert!(events.iter().any(|event| event["event"]["name"] == "usage"));
    let missing = service
        .get(&format!("/api/threads/{thread}/items/none"))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), 404);
}

#[tokio::test]
async fn edits_detect_conflicts_and_create_only_when_absent() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    let (code, created) = write(
        &service,
        &thread,
        json!({ "path": "src/a.txt", "content": "one", "base_revision": null }),
    )
    .await;
    assert_eq!(code, 200, "{created}");
    let revision = created["revision"].as_str().unwrap().to_owned();

    // Creating over an existing file is a conflict, not an overwrite.
    let (code, body) = write(
        &service,
        &thread,
        json!({ "path": "src/a.txt", "content": "two", "base_revision": null }),
    )
    .await;
    assert_eq!(code, 409);
    assert_eq!(body["code"], "file_conflict");
    assert_eq!(body["detail"]["revision"], revision.as_str());

    // Someone else moves the file on, as an engine would.
    let file = dir.path().join("library/repo/project/src/a.txt");
    std::fs::write(&file, "engine edit").unwrap();
    let (code, _) = write(
        &service,
        &thread,
        json!({ "path": "src/a.txt", "content": "mine", "base_revision": revision }),
    )
    .await;
    assert_eq!(code, 409);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "engine edit");

    let (_, current) = read(&service, &thread, "src/a.txt").await;
    assert_eq!(current["content"], "engine edit");
    let (code, _) = write(
        &service,
        &thread,
        json!({ "path": "src/a.txt", "content": "mine", "base_revision": current["revision"] }),
    )
    .await;
    assert_eq!(code, 200);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "mine");
    let leftovers = std::fs::read_dir(file.parent().unwrap()).unwrap().count();
    assert_eq!(leftovers, 1, "no partial save remains");
    assert!(
        service
            .audit(&thread)
            .await
            .iter()
            .any(|row| row["action"] == "file.write")
    );
}

#[tokio::test]
async fn paths_cannot_leave_the_checkout_or_touch_git_metadata() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        dir.path().join("library"),
        dir.path().join("library/repo/project/escape"),
    )
    .unwrap();
    for path in [
        "../../AGENTS.md",
        ".git/config",
        "/etc/hosts",
        "escape/AGENTS.md",
    ] {
        if path.starts_with("escape") && cfg!(not(unix)) {
            continue;
        }
        let (code, _) = read(&service, &thread, path).await;
        assert_eq!(code, 400, "read {path}");
        let (code, _) = write(
            &service,
            &thread,
            json!({ "path": path, "content": "x", "base_revision": null }),
        )
        .await;
        assert_eq!(code, 400, "write {path}");
    }
    assert_eq!(
        std::fs::read_to_string(dir.path().join("library/AGENTS.md")).unwrap(),
        "shared policy\n"
    );
}

#[tokio::test]
async fn untrusted_folders_are_not_browsed() {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), Options::default());
    let folder = dir.path().join("project");
    let project = service.project(&folder, false).await;
    let thread = service.thread(&project, "codex").await;
    let response = service
        .get(&format!("/api/threads/{thread}/files"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
}

#[tokio::test]
async fn tree_marks_ignored_and_search_finds_names_and_lines() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    let checkout = dir.path().join("library/repo/project");
    std::fs::write(checkout.join(".gitignore"), "build/\n").unwrap();
    std::fs::create_dir_all(checkout.join("build")).unwrap();
    std::fs::write(checkout.join("build/out.bin"), "needle").unwrap();
    std::fs::write(checkout.join("main.rs"), "fn main() {}\n// Needle: here\n").unwrap();
    run_git(&checkout, &["add", "main.rs"]);

    let tree = service.json(&format!("/api/threads/{thread}/files")).await;
    let entry = |name: &str| {
        tree.as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == name)
            .unwrap()
            .clone()
    };
    assert_eq!(entry("build")["ignored"], true);
    assert_eq!(entry("build")["is_dir"], true);
    assert_eq!(entry("main.rs")["ignored"], false);
    assert!(
        tree.as_array()
            .unwrap()
            .iter()
            .all(|row| row["name"] != ".git")
    );

    let ignored = service
        .json(&format!("/api/threads/{thread}/files/ignored"))
        .await;
    assert_eq!(ignored, json!(["build/"]));

    let names = service
        .json(&format!("/api/threads/{thread}/files/search?q=MAIN"))
        .await;
    assert_eq!(names[0]["path"], "main.rs");
    let lines = service
        .json(&format!(
            "/api/threads/{thread}/files/search?q=needle&content=true"
        ))
        .await;
    assert_eq!(
        lines.as_array().unwrap().len(),
        1,
        "ignored files stay out: {lines}"
    );
    assert_eq!(lines[0]["line"], 2);
    let none = service
        .json(&format!(
            "/api/threads/{thread}/files/search?q=absent&content=true"
        ))
        .await;
    assert!(none.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn metadata_describes_a_file_a_folder_and_the_root_without_their_bytes() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    let root = checkout(&service, &thread).await;
    std::fs::write(root.join("notes.txt"), "hello").unwrap();
    std::fs::create_dir_all(root.join("nested")).unwrap();

    let (code, file) = metadata(&service, &thread, "notes.txt").await;
    assert_eq!(code, 200, "{file}");
    assert_eq!(file["path"], "notes.txt");
    assert_eq!(file["exists"], true);
    assert_eq!(file["kind"], "file");
    assert_eq!(file["size"], 5);
    assert_eq!(file["inside"], true);
    let modified = file["modified"].as_str().unwrap();
    assert!(modified.ends_with('Z'), "{modified}");

    let (code, folder) = metadata(&service, &thread, "nested").await;
    assert_eq!(code, 200, "{folder}");
    assert_eq!(folder["exists"], true);
    assert_eq!(folder["kind"], "directory");
    assert_eq!(folder["inside"], true);

    // The empty path is the checkout's root, as it is for the folder listing.
    let (code, at_root) = metadata(&service, &thread, "").await;
    assert_eq!(code, 200, "{at_root}");
    assert_eq!(at_root["kind"], "directory");
    assert_eq!(at_root["inside"], true);
}

#[tokio::test]
async fn metadata_answers_a_missing_path_and_refuses_one_outside_the_checkout() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    let outside = dir.path().join("outside.txt");
    std::fs::write(&outside, "secret").unwrap();

    // Nothing there is an answer the file browser can draw, not an error.
    for path in ["missing.txt", "missing/folder.txt"] {
        let (code, body) = metadata(&service, &thread, path).await;
        assert_eq!(code, 200, "{path}: {body}");
        assert_eq!(body["exists"], false, "{path}");
        assert_eq!(body["inside"], true, "{path}");
        assert!(body["kind"].is_null(), "{path}");
        assert_eq!(body["size"], 0, "{path}");
    }

    // The same boundary as any other file read: no dot-dot, no full path, no Git's folder.
    let absolute = outside.to_string_lossy().into_owned();
    for path in [
        "../outside.txt",
        "../../outside.txt",
        ".git/config",
        ".GIT/config",
        "nested/.git/config",
        "missing/../two.txt",
        absolute.as_str(),
    ] {
        let (code, body) = metadata(&service, &thread, path).await;
        assert_eq!(code, 400, "{path}: {body}");
    }
}

#[tokio::test]
async fn metadata_needs_a_trusted_folder() {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), Options::default());
    let folder = dir.path().join("project");
    std::fs::create_dir_all(&folder).unwrap();
    let project = service.project(&folder, false).await;
    let thread = service.thread(&project, "codex").await;
    let (code, _) = metadata(&service, &thread, "a.txt").await;
    assert_eq!(code, 403);
}

/// A Git repository on `main` at `root`, with one commit holding `files`.
fn repository(root: &std::path::Path, files: &[(&str, &str)]) -> std::path::PathBuf {
    std::fs::create_dir_all(root).unwrap();
    run_git(root, &["init", "-q", "-b", "main"]);
    run_git(root, &["config", "user.email", "t@example.com"]);
    run_git(root, &["config", "user.name", "T"]);
    for (name, content) in files {
        std::fs::write(root.join(name), content).unwrap();
    }
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-q", "-m", "first"]);
    root.to_path_buf()
}

/// The entries of a thread's file tree, for `repo`: absent is the primary repository.
async fn tree(service: &Service, thread: &str, repo: Option<&str>) -> (u16, Value) {
    let mut request = service.get(&format!("/api/threads/{thread}/files"));
    if let Some(repo) = repo {
        request = request.query(&[("repo_id", repo)]);
    }
    let response = request.send().await.unwrap();
    let status = response.status().as_u16();
    (status, response.json().await.unwrap_or(Value::Null))
}

/// One path's metadata, for `repo`: absent is the primary repository.
async fn metadata_in(
    service: &Service,
    thread: &str,
    path: &str,
    repo: Option<&str>,
) -> (u16, Value) {
    let mut request = service
        .get(&format!("/api/threads/{thread}/files/metadata"))
        .query(&[("path", path)]);
    if let Some(repo) = repo {
        request = request.query(&[("repo_id", repo)]);
    }
    let response = request.send().await.unwrap();
    let status = response.status().as_u16();
    (status, response.json().await.unwrap_or(Value::Null))
}

/// One file's content, for `repo`: absent is the primary repository.
async fn content_in(
    service: &Service,
    thread: &str,
    path: &str,
    repo: Option<&str>,
) -> (u16, Value) {
    let mut request = service
        .get(&format!("/api/threads/{thread}/files/content"))
        .query(&[("path", path)]);
    if let Some(repo) = repo {
        request = request.query(&[("repo_id", repo)]);
    }
    let response = request.send().await.unwrap();
    let status = response.status().as_u16();
    (status, response.json().await.unwrap_or(Value::Null))
}

/// One file search of a thread's checkout, for `repo`: absent is the primary repository.
async fn search_in(
    service: &Service,
    thread: &str,
    query: &str,
    content: bool,
    repo: Option<&str>,
) -> (u16, Value) {
    let mut request = service
        .get(&format!("/api/threads/{thread}/files/search"))
        .query(&[
            ("q", query),
            ("content", if content { "true" } else { "false" }),
        ]);
    if let Some(repo) = repo {
        request = request.query(&[("repo_id", repo)]);
    }
    let response = request.send().await.unwrap();
    let status = response.status().as_u16();
    (status, response.json().await.unwrap_or(Value::Null))
}

/// The paths of a search answer, in order.
fn paths(body: Value) -> Vec<String> {
    body.as_array()
        .unwrap()
        .iter()
        .map(|hit| hit["path"].as_str().unwrap().to_owned())
        .collect()
}

/// The file names of a tree answer, in order; anything but `200` fails here.
fn names((status, body): (u16, Value)) -> Vec<String> {
    assert_eq!(status, 200, "{body}");
    body.as_array()
        .unwrap()
        .iter()
        .map(|row| row["name"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn a_repo_id_reads_the_named_repositories_tree_and_metadata() {
    let dir = TempDir::new();
    // A multi-repository thread needs the beta flag on.
    let service = Service::start(
        &dir.path().join("data"),
        Options {
            env: vec![("ORBYN_MULTI_REPO".to_owned(), "1".to_owned())],
            ..Options::default()
        },
    );
    service.ensure_workspace(dir.path()).await;
    let primary = repository(
        &dir.path().join("work/primary"),
        &[("base.txt", "base\n"), ("same.txt", "one\n")],
    );
    let other = repository(
        &dir.path().join("work/other"),
        &[("other.txt", "other\n"), ("same.txt", "other repo\n")],
    );
    let (code, project) = service
        .send("/api/projects/add", json!({ "path": primary }))
        .await;
    assert_eq!(code, 201, "{project}");
    let project_id = project["id"].as_str().unwrap().to_owned();
    let (code, _) = service
        .send(&format!("/api/projects/{project_id}/trust"), json!({}))
        .await;
    assert_eq!(code, 200);
    let (code, added) = service
        .send(
            &format!("/api/projects/{project_id}/repos/add-folder"),
            json!({ "path": other }),
        )
        .await;
    assert_eq!(code, 200, "{added}");
    let other_id = added["id"].as_str().unwrap().to_owned();
    assert_eq!(added["trusted"], false);

    // No route can put an untrusted repository in a thread: the start refuses it here,
    // and adding one to a running thread is refused the same way. So the file routes'
    // own untrusted gate (`scoped_thread` -> `409 repo_untrusted`) runs on state no
    // route can build; the core test
    // `an_unknown_untrusted_or_flagged_off_repository_is_refused_at_every_entry` asserts it.
    let start = json!({
        "repos": [
            { "repo_id": project_id, "base_branch": "main" },
            { "repo_id": other_id, "base_branch": "main" },
        ],
        "engine": "codex",
        "model": null,
        "permission_mode": "supervised",
    });
    let (code, refused) = service
        .send(
            &format!("/api/projects/{project_id}/repo-threads"),
            start.clone(),
        )
        .await;
    assert_eq!(code, 409, "{refused}");
    assert_eq!(refused["code"], "repo_untrusted", "{refused}");

    let (code, trusted) = service
        .send(
            &format!("/api/projects/{project_id}/repos/{other_id}/trust"),
            json!({}),
        )
        .await;
    assert_eq!(code, 200, "{trusted}");
    assert_eq!(trusted["trusted"], true);

    // Two repositories make a task folder: one worktree per repository, one branch.
    let (code, thread) = service
        .send(&format!("/api/projects/{project_id}/repo-threads"), start)
        .await;
    assert_eq!(code, 200, "{thread}");
    let thread = thread["id"].as_str().unwrap().to_owned();

    // No repo_id is the primary repository, and naming it explicitly reads the same
    // worktree. Each repository has a file of its own, so a tree is one repository's.
    assert_eq!(
        names(tree(&service, &thread, None).await),
        ["base.txt", "same.txt"]
    );
    assert_eq!(
        names(tree(&service, &thread, Some(&project_id)).await),
        ["base.txt", "same.txt"]
    );
    assert_eq!(
        names(tree(&service, &thread, Some(&other_id)).await),
        ["other.txt", "same.txt"]
    );

    // Metadata resolves the path inside the named repository's worktree only, so the
    // same file name is two files and each size is the one on that repository's disk.
    let folder = service
        .json(&format!("/api/threads/{thread}/task-folder"))
        .await;
    let root = std::path::PathBuf::from(folder["root"].as_str().unwrap());
    let worktree = |repo: &str| {
        let name = folder["repos"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["repo_id"] == repo)
            .unwrap()["name"]
            .as_str()
            .unwrap();
        root.join(name)
    };
    let on_disk = |repo: &str| {
        std::fs::metadata(worktree(repo).join("same.txt"))
            .unwrap()
            .len()
    };
    let (code, primary_same) = metadata_in(&service, &thread, "same.txt", None).await;
    assert_eq!(code, 200, "{primary_same}");
    assert_eq!(primary_same["size"], on_disk(&project_id));
    let (code, other_same) = metadata_in(&service, &thread, "same.txt", Some(&other_id)).await;
    assert_eq!(code, 200, "{other_same}");
    assert_eq!(other_same["size"], on_disk(&other_id));
    assert_ne!(primary_same["size"], other_same["size"]);
    assert_eq!(other_same["kind"], "file");
    // A file only the other repository has is absent from the primary read.
    let (code, absent) = metadata_in(&service, &thread, "other.txt", None).await;
    assert_eq!(code, 200, "{absent}");
    assert_eq!(absent["exists"], false);
    let (code, found) = metadata_in(&service, &thread, "other.txt", Some(&other_id)).await;
    assert_eq!(code, 200, "{found}");
    assert_eq!(found["exists"], true);

    // A repository that is not the thread's is not found, as on the Git routes.
    assert_eq!(tree(&service, &thread, Some("no-such")).await.0, 404);
    assert_eq!(
        metadata_in(&service, &thread, "same.txt", Some("no-such"))
            .await
            .0,
        404
    );

    // An ordinary thread has one checkout and no repositories, so any repo_id is 404.
    let plain = service.thread(&project_id, "codex").await;
    assert_eq!(
        names(tree(&service, &plain, None).await),
        ["base.txt", "same.txt"]
    );
    assert_eq!(tree(&service, &plain, Some(&other_id)).await.0, 404);
    assert_eq!(
        metadata_in(&service, &plain, "same.txt", Some(&other_id))
            .await
            .0,
        404
    );
}

/// Hunk revert, its undo and the inline line edit are one read and one write of a file
/// (`docs/multi-repo.md`, "File reads and writes name their repository"): both take `repo_id`,
/// so the same relative path is never read from, or written to, another repository's worktree.
#[tokio::test]
async fn a_repo_id_names_the_repository_a_file_read_and_write() {
    let fixture = repo_thread().await;
    // The same file name with its own text in each worktree. Untracked files need no commit,
    // and the write a hunk edit makes is the whole file, as this one is.
    std::fs::write(fixture.primary_root.join("same.txt"), "primary\n").unwrap();
    std::fs::write(fixture.other_root.join("same.txt"), "other\n").unwrap();

    // No repo_id is the primary repository, and naming it explicitly reads the same file.
    for repo in [None, Some(fixture.project.as_str())] {
        let (code, body) = content_in(&fixture.service, &fixture.thread, "same.txt", repo).await;
        assert_eq!(code, 200, "{repo:?}: {body}");
        assert_eq!(body["content"], "primary\n", "{repo:?}");
    }
    // The other repository's file is that repository's, at the same relative path.
    let (code, other) = content_in(
        &fixture.service,
        &fixture.thread,
        "same.txt",
        Some(&fixture.other),
    )
    .await;
    assert_eq!(code, 200, "{other}");
    assert_eq!(other["content"], "other\n");

    // A hunk revert or line edit writes the file back through this route: the write lands in
    // the named repository only, with the revision read from that repository.
    let (code, saved) = write(
        &fixture.service,
        &fixture.thread,
        json!({
            "path": "same.txt",
            "content": "edited in the other\n",
            "base_revision": other["revision"],
            "repo_id": fixture.other.clone(),
        }),
    )
    .await;
    assert_eq!(code, 200, "{saved}");
    assert_eq!(
        std::fs::read_to_string(fixture.other_root.join("same.txt")).unwrap(),
        "edited in the other\n"
    );
    assert_eq!(
        std::fs::read_to_string(fixture.primary_root.join("same.txt")).unwrap(),
        "primary\n",
        "the primary repository's file is untouched"
    );

    // A revision belongs to the repository's file: the other repository's revision on the same
    // path is a changed file, not a silent overwrite of the primary.
    let (code, refused) = write(
        &fixture.service,
        &fixture.thread,
        json!({
            "path": "same.txt",
            "content": "mine\n",
            "base_revision": saved["revision"],
            "repo_id": fixture.project.clone(),
        }),
    )
    .await;
    assert_eq!(code, 409, "{refused}");
    assert_eq!(refused["code"], "file_conflict", "{refused}");
    assert_eq!(
        std::fs::read_to_string(fixture.other_root.join("same.txt")).unwrap(),
        "edited in the other\n"
    );

    // A path that leaves the named repository is refused even when the same spelling lands in
    // a sibling worktree: no request falls back to another repository's checkout.
    let primary_name = fixture
        .primary_root
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let escaped = format!("../{primary_name}/same.txt");
    let (code, refused) = content_in(
        &fixture.service,
        &fixture.thread,
        &escaped,
        Some(&fixture.other),
    )
    .await;
    assert_eq!(code, 400, "{refused}");
    let (code, refused) = write(
        &fixture.service,
        &fixture.thread,
        json!({
            "path": escaped,
            "content": "x\n",
            "base_revision": null,
            "repo_id": fixture.other.clone(),
        }),
    )
    .await;
    assert_eq!(code, 400, "{refused}");

    // A repository that is not the thread's, and any repo_id on an ordinary thread, is not found.
    let (code, unknown) = content_in(
        &fixture.service,
        &fixture.thread,
        "same.txt",
        Some("no-such-repository"),
    )
    .await;
    assert_eq!(code, 404, "{unknown}");
    assert_eq!(unknown["code"], "not_found", "{unknown}");
    let plain = fixture.service.thread(&fixture.project, "codex").await;
    let (code, refused) =
        content_in(&fixture.service, &plain, "same.png", Some(&fixture.project)).await;
    assert_eq!(code, 404, "{refused}");
    // Only the repo_id is refused: the ordinary thread's own read is unchanged.
    let (code, _) = content_in(&fixture.service, &plain, "same.png", None).await;
    assert_eq!(code, 200);
}

/// The command palette's "Go to file" and "Search in project" are one file search of a thread's
/// checkout (`docs/multi-repo.md`): it takes `repo_id` too, so a multi-repository thread searches
/// the repository its chip names and never another repository's same-named files.
#[tokio::test]
async fn a_repo_id_names_the_repository_a_file_search() {
    let fixture = repo_thread().await;
    // A name each worktree alone holds, so an answer says which repository was searched.
    std::fs::write(
        fixture.primary_root.join("primary-only.txt"),
        "in the primary\n",
    )
    .unwrap();
    std::fs::write(fixture.other_root.join("other-only.txt"), "in the other\n").unwrap();

    // No repo_id is the primary repository, and naming it explicitly searches the same worktree.
    for repo in [None, Some(fixture.project.as_str())] {
        let (code, found) = search_in(&fixture.service, &fixture.thread, "only", false, repo).await;
        assert_eq!(code, 200, "{repo:?}: {found}");
        assert_eq!(paths(found), vec!["primary-only.txt"], "{repo:?}");
    }
    // The other repository's file is found only when its repo_id names it.
    let (code, found) = search_in(
        &fixture.service,
        &fixture.thread,
        "only",
        false,
        Some(&fixture.other),
    )
    .await;
    assert_eq!(code, 200, "{found}");
    assert_eq!(paths(found), vec!["other-only.txt"]);

    // A search of lines follows the same repository: the other worktree holds the text.
    let (code, found) = search_in(
        &fixture.service,
        &fixture.thread,
        "in the other",
        true,
        Some(&fixture.other),
    )
    .await;
    assert_eq!(code, 200, "{found}");
    assert_eq!(paths(found), vec!["other-only.txt"]);
    let (code, found) = search_in(
        &fixture.service,
        &fixture.thread,
        "in the other",
        true,
        None,
    )
    .await;
    assert_eq!(code, 200, "{found}");
    let listed = paths(found);
    assert!(
        listed.is_empty(),
        "the primary has no such line: {listed:?}"
    );

    // A repository that is not the thread's, and any repo_id on an ordinary thread, is not found.
    let (code, refused) = search_in(
        &fixture.service,
        &fixture.thread,
        "only",
        false,
        Some("no-such-repository"),
    )
    .await;
    assert_eq!(code, 404, "{refused}");
    assert_eq!(refused["code"], "not_found", "{refused}");
    let plain = fixture.service.thread(&fixture.project, "codex").await;
    let (code, refused) = search_in(
        &fixture.service,
        &plain,
        "same.png",
        false,
        Some(&fixture.project),
    )
    .await;
    assert_eq!(code, 404, "{refused}");
    // Only the repo_id is refused: the ordinary thread's own search is unchanged.
    let (code, own) = search_in(&fixture.service, &plain, "same.png", false, None).await;
    assert_eq!(code, 200, "{own}");
    assert_eq!(paths(own), vec!["same.png"]);
}

#[cfg(unix)]
#[tokio::test]
async fn metadata_reports_permission_bits_and_a_link_as_itself() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    let root = checkout(&service, &thread).await;
    let elsewhere = dir.path().join("elsewhere.txt");
    assert!(
        !elsewhere.starts_with(&root),
        "the fixture needs a file outside the checkout"
    );

    let script = root.join("run.sh");
    std::fs::write(&script, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o640)).unwrap();
    let (code, body) = metadata(&service, &thread, "run.sh").await;
    assert_eq!(code, 200, "{body}");
    assert_eq!(body["mode"].as_u64(), Some(0o640));
    assert_eq!(body["read_only"], false);
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o444)).unwrap();
    let (_, body) = metadata(&service, &thread, "run.sh").await;
    assert_eq!(body["read_only"], true);

    // A link to a file inside is reported as a link, never as what it points at.
    symlink("run.sh", root.join("inside-link")).unwrap();
    let (code, body) = metadata(&service, &thread, "inside-link").await;
    assert_eq!(code, 200, "{body}");
    assert_eq!(body["kind"], "symlink");
    assert_eq!(body["size"].as_u64(), Some("run.sh".len() as u64));
    assert_eq!(body["inside"], true);

    // A link out of the checkout answers as the link with inside false, and its size is
    // the link's own text: nothing about the target, its size or its contents.
    std::fs::write(&elsewhere, "very secret bytes").unwrap();
    let target = elsewhere.to_string_lossy().into_owned();
    symlink(&elsewhere, root.join("outside-link")).unwrap();
    let (code, body) = metadata(&service, &thread, "outside-link").await;
    assert_eq!(code, 200, "{body}");
    assert_eq!(body["kind"], "symlink");
    assert_eq!(body["exists"], true);
    assert_eq!(body["inside"], false);
    assert_eq!(body["size"].as_u64(), Some(target.len() as u64));
    assert!(
        body.get("target").is_none(),
        "never where a link points: {body}"
    );
    assert!(body.get("content").is_none(), "never any bytes: {body}");

    // A link into Git's folder is described the same way, and its target is not reached.
    symlink(".git/config", root.join("git-link")).unwrap();
    let (code, body) = metadata(&service, &thread, "git-link").await;
    assert_eq!(code, 200, "{body}");
    assert_eq!(body["kind"], "symlink");
    assert_eq!(body["inside"], false);

    // A link walked through as a folder cannot lead a read out of the checkout.
    let escape = dir.path().join("escape");
    std::fs::create_dir_all(&escape).unwrap();
    std::fs::write(escape.join("hidden.txt"), "hidden").unwrap();
    symlink(&escape, root.join("escape-link")).unwrap();
    let (code, body) = metadata(&service, &thread, "escape-link/hidden.txt").await;
    assert_eq!(code, 400, "{body}");
}

#[cfg(unix)]
#[tokio::test]
async fn metadata_answers_a_pipe_without_opening_it() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    let root = checkout(&service, &thread).await;
    let pipe = root.join("pipe");
    let made = std::process::Command::new("mkfifo")
        .arg(&pipe)
        .status()
        .unwrap();
    assert!(made.success());

    // Opening a pipe for reading blocks until someone writes; metadata reads no bytes,
    // so the answer must come back on its own.
    let answer = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        metadata(&service, &thread, "pipe"),
    )
    .await
    .expect("a metadata read never opens the file, so a pipe cannot block it");
    assert_eq!(answer.0, 200, "{}", answer.1);
    assert_eq!(answer.1["exists"], true);
    assert_eq!(answer.1["inside"], true);
    assert!(
        answer.1["kind"].is_null(),
        "a pipe is not a file, folder or link: {}",
        answer.1
    );
}

#[cfg(unix)]
#[tokio::test]
async fn open_in_editor_runs_only_a_known_editor_on_a_checkout_file() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new();
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let record = dir.path().join("opened");
    let code = bin.join("code");
    std::fs::write(
        &code,
        format!("#!/bin/sh\necho \"$1\" > '{}'\n", record.display()),
    )
    .unwrap();
    std::fs::set_permissions(&code, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let (service, thread) = setup(
        &dir,
        Options {
            env: vec![("PATH".into(), path)],
            ..Options::default()
        },
        "codex",
    )
    .await;
    assert!(
        service
            .json("/api/editors")
            .await
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["id"] == "vscode")
    );
    let checkout = dir.path().join("library/repo/project");
    std::fs::write(checkout.join("a.txt"), "x").unwrap();
    let opened = format!("/api/threads/{thread}/files/open");
    let (status, _) = service
        .send(
            &opened,
            json!({ "path": "a.txt", "editor": "not-an-editor" }),
        )
        .await;
    assert_eq!(status, 400);
    let (status, _) = service
        .send(
            &opened,
            json!({ "path": "../../AGENTS.md", "editor": "vscode" }),
        )
        .await;
    assert_eq!(status, 400);
    assert!(!record.exists());
    let (status, _) = service
        .send(&opened, json!({ "path": "a.txt", "editor": "vscode" }))
        .await;
    assert_eq!(status, 202);
    // The editor runs detached, and on a busy machine it can take a while.
    let mut seen = String::new();
    for _ in 0..6000 {
        seen = std::fs::read_to_string(&record).unwrap_or_default();
        if !seen.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert_eq!(seen.trim(), checkout.join("a.txt").to_str().unwrap());
}

async fn raw(service: &Service, thread: &str, path: &str) -> reqwest::RequestBuilder {
    service
        .get(&format!("/api/threads/{thread}/files/raw"))
        .query(&[("path", path)])
}

fn header<'a>(response: &'a reqwest::Response, name: &str) -> &'a str {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
}

async fn checkout(service: &Service, thread: &str) -> std::path::PathBuf {
    let body = service.json(&format!("/api/threads/{thread}")).await;
    body["cwd"].as_str().unwrap().into()
}

#[tokio::test]
async fn raw_bytes_serve_ranges_and_never_run_a_page_in_this_origin() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    let root = checkout(&service, &thread).await;
    let bytes: Vec<u8> = (0..=255).collect();
    std::fs::write(root.join("clip.mp4"), &bytes).unwrap();
    std::fs::write(root.join("page.html"), "<script>1</script>").unwrap();
    std::fs::write(root.join("mark.svg"), "<svg/>").unwrap();
    std::fs::write(root.join("doc.pdf"), "%PDF").unwrap();

    let whole = raw(&service, &thread, "clip.mp4")
        .await
        .send()
        .await
        .unwrap();
    assert_eq!(whole.status(), 200);
    assert_eq!(header(&whole, "content-type"), "video/mp4");
    assert_eq!(header(&whole, "accept-ranges"), "bytes");
    assert_eq!(header(&whole, "x-content-type-options"), "nosniff");
    assert!(header(&whole, "content-disposition").starts_with("inline"));
    let tag = header(&whole, "etag").to_owned();
    assert_eq!(whole.bytes().await.unwrap().as_ref(), &bytes[..]);

    let part = raw(&service, &thread, "clip.mp4")
        .await
        .header("Range", "bytes=10-19")
        .send()
        .await
        .unwrap();
    assert_eq!(part.status(), 206);
    assert_eq!(header(&part, "content-range"), "bytes 10-19/256");
    assert_eq!(header(&part, "content-length"), "10");
    assert_eq!(part.bytes().await.unwrap().as_ref(), &bytes[10..20]);

    let tail = raw(&service, &thread, "clip.mp4")
        .await
        .header("Range", "bytes=-6")
        .send()
        .await
        .unwrap();
    assert_eq!(tail.bytes().await.unwrap().as_ref(), &bytes[250..]);

    let past = raw(&service, &thread, "clip.mp4")
        .await
        .header("Range", "bytes=256-")
        .send()
        .await
        .unwrap();
    assert_eq!(past.status(), 416);
    assert_eq!(header(&past, "content-range"), "bytes */256");

    // A file that changed answers in full, so two versions are never stitched.
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(root.join("clip.mp4"), b"newer").unwrap();
    let stale = raw(&service, &thread, "clip.mp4")
        .await
        .header("Range", "bytes=0-1")
        .header("If-Range", &tag)
        .send()
        .await
        .unwrap();
    assert_eq!(stale.status(), 200);
    assert_eq!(stale.bytes().await.unwrap().as_ref(), b"newer");

    let fresh = raw(&service, &thread, "clip.mp4")
        .await
        .send()
        .await
        .unwrap();
    let tag = header(&fresh, "etag").to_owned();
    let same = raw(&service, &thread, "clip.mp4")
        .await
        .header("If-None-Match", &tag)
        .send()
        .await
        .unwrap();
    assert_eq!(same.status(), 304);

    let page = raw(&service, &thread, "page.html")
        .await
        .send()
        .await
        .unwrap();
    assert_eq!(header(&page, "content-type"), "application/octet-stream");
    assert!(header(&page, "content-disposition").starts_with("attachment"));

    let svg = raw(&service, &thread, "mark.svg")
        .await
        .send()
        .await
        .unwrap();
    assert_eq!(header(&svg, "content-type"), "image/svg+xml");
    assert!(header(&svg, "content-security-policy").contains("sandbox"));

    let pdf = raw(&service, &thread, "doc.pdf")
        .await
        .send()
        .await
        .unwrap();
    assert_eq!(header(&pdf, "content-type"), "application/pdf");
    assert_eq!(header(&pdf, "content-security-policy"), "");

    let forced = service
        .get(&format!("/api/threads/{thread}/files/raw"))
        .query(&[("path", "mark.svg"), ("download", "true")])
        .send()
        .await
        .unwrap();
    assert!(header(&forced, "content-disposition").starts_with("attachment"));
}

#[tokio::test]
async fn raw_bytes_stay_inside_the_checkout_and_behind_the_token() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    let root = checkout(&service, &thread).await;
    std::fs::write(dir.path().join("outside.png"), "secret").unwrap();
    std::fs::write(root.join("in.png"), "ok").unwrap();
    std::fs::create_dir_all(root.join("sub")).unwrap();

    for path in [
        "../outside.png",
        "sub/../../outside.png",
        "/etc/hosts",
        ".git/config",
        ".GIT/config",
    ] {
        let response = raw(&service, &thread, path).await.send().await.unwrap();
        assert_eq!(response.status(), 400, "{path}");
    }
    let folder = raw(&service, &thread, "sub").await.send().await.unwrap();
    assert_eq!(folder.status(), 400);
    let missing = raw(&service, &thread, "none.png")
        .await
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), 404);

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(dir.path().join("outside.png"), root.join("link.png")).unwrap();
        std::os::unix::fs::symlink(".git/config", root.join("cfg.png")).unwrap();
        for path in ["link.png", "cfg.png"] {
            let linked = raw(&service, &thread, path).await.send().await.unwrap();
            assert_eq!(linked.status(), 400, "{path}");
        }
    }

    // An image element cannot set a header, so this route takes the token in the URL,
    // and no other route gains that.
    let url = format!("{}/api/threads/{thread}/files/raw", service.url);
    let anonymous = service
        .client
        .get(&url)
        .query(&[("path", "in.png")])
        .send()
        .await
        .unwrap();
    assert_eq!(anonymous.status(), 401);
    let wrong = service
        .client
        .get(&url)
        .query(&[("path", "in.png"), ("token", "wrong")])
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), 401);
    let good = service
        .client
        .get(&url)
        .query(&[("path", "in.png"), ("token", service.token.as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(good.status(), 200);
    let other = service
        .client
        .get(format!(
            "{}/api/threads/{thread}/files/content",
            service.url
        ))
        .query(&[("path", "in.png"), ("token", service.token.as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(other.status(), 401);
}

#[tokio::test]
async fn raw_bytes_need_a_trusted_folder() {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), Options::default());
    let folder = dir.path().join("project");
    std::fs::create_dir_all(&folder).unwrap();
    let project = service.project(&folder, false).await;
    let thread = service.thread(&project, "codex").await;
    let response = raw(&service, &thread, "a.png").await.send().await.unwrap();
    assert_eq!(response.status(), 403);
}

#[tokio::test]
async fn raw_bytes_refuse_a_file_past_the_limit() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    let root = checkout(&service, &thread).await;
    // A sparse file, so this takes no disk.
    let huge = std::fs::File::create(root.join("huge.mp4")).unwrap();
    huge.set_len(orbyn_core::project::file_bytes::MAX_RAW_BYTES + 1)
        .unwrap();
    let response = raw(&service, &thread, "huge.mp4")
        .await
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 413);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["code"], "file_too_large");
}

async fn outside(service: &Service, thread: &str, path: &str) -> (u16, Value) {
    let response = service
        .get(&format!("/api/threads/{thread}/files/outside"))
        .query(&[("path", path)])
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    (status, response.json().await.unwrap_or(Value::Null))
}

fn full(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}

#[tokio::test]
async fn an_outside_file_is_shown_read_only_and_never_written() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    let root = checkout(&service, &thread).await;
    let elsewhere = dir.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let page = elsewhere.join("report.html");
    std::fs::write(&page, "<script>parent.x=1</script><p>hi</p>").unwrap();

    let (code, body) = outside(&service, &thread, &full(&page)).await;
    assert_eq!(code, 200, "{body}");
    assert_eq!(body["outside"], true);
    assert_eq!(body["read_only"], true);
    assert_eq!(body["content"], "<script>parent.x=1</script><p>hi</p>");
    assert!(body.get("revision").is_none(), "nothing to save against");

    // No route writes it: the checkout save refuses a full path, and the outside
    // route has no write method.
    let (code, _) = write(
        &service,
        &thread,
        json!({ "path": full(&page), "content": "pwned", "base_revision": null }),
    )
    .await;
    assert_eq!(code, 400);
    let put = service
        .client
        .put(format!(
            "{}/api/threads/{thread}/files/outside",
            service.url
        ))
        .query(&[("path", full(&page))])
        .bearer_auth(&service.token)
        .json(&json!({ "content": "pwned" }))
        .send()
        .await
        .unwrap();
    assert_eq!(put.status(), 405);
    assert_eq!(
        std::fs::read_to_string(&page).unwrap(),
        "<script>parent.x=1</script><p>hi</p>"
    );

    // A folder outside is neither read nor listed, by either route.
    let (code, _) = outside(&service, &thread, &full(&elsewhere)).await;
    assert_eq!(code, 400);
    let listing = service
        .get(&format!("/api/threads/{thread}/files"))
        .query(&[("path", full(&elsewhere))])
        .send()
        .await
        .unwrap();
    assert_eq!(listing.status(), 400);

    // A relative path names nothing here; `..` lands where it lands.
    let (code, _) = outside(&service, &thread, "../elsewhere/report.html").await;
    assert_eq!(code, 400);
    std::fs::write(root.join("inside.txt"), "in").unwrap();
    let dotted = root
        .join("..")
        .join(root.file_name().unwrap())
        .join("inside.txt");
    let (code, body) = outside(&service, &thread, &full(&dotted)).await;
    assert_eq!(code, 400, "{body}");

    // Orbyn's own data, which holds the token, is never shown.
    let data = dir.path().join("data");
    let any = std::fs::read_dir(&data)
        .unwrap()
        .flatten()
        .find(|entry| entry.path().is_file())
        .unwrap()
        .path();
    let (code, _) = outside(&service, &thread, &full(&any)).await;
    assert_eq!(code, 400);

    // Missing is a 404 and the cap is the checkout read's 4 MiB.
    let (code, _) = outside(&service, &thread, &full(&elsewhere.join("gone.txt"))).await;
    assert_eq!(code, 404);
    let big = elsewhere.join("big.log");
    std::fs::File::create(&big)
        .unwrap()
        .set_len(4 * 1024 * 1024 + 1)
        .unwrap();
    let (code, body) = outside(&service, &thread, &full(&big)).await;
    assert_eq!(code, 200);
    assert_eq!(body["truncated"], true);
    assert_eq!(body["content"], "");
}

#[cfg(unix)]
#[tokio::test]
async fn a_link_outside_is_judged_where_it_lands() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    let root = checkout(&service, &thread).await;
    std::fs::write(root.join("secret.txt"), "in").unwrap();
    let elsewhere = dir.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::write(elsewhere.join("real.txt"), "out").unwrap();
    std::os::unix::fs::symlink(root.join("secret.txt"), elsewhere.join("into")).unwrap();
    std::os::unix::fs::symlink(dir.path().join("data"), elsewhere.join("data")).unwrap();
    std::os::unix::fs::symlink(elsewhere.join("real.txt"), elsewhere.join("ok")).unwrap();
    let (code, _) = outside(&service, &thread, &full(&elsewhere.join("into"))).await;
    assert_eq!(code, 400);
    let token = std::fs::read_dir(dir.path().join("data"))
        .unwrap()
        .flatten()
        .find(|entry| entry.path().is_file())
        .unwrap()
        .file_name();
    let through = elsewhere.join("data").join(token);
    let (code, _) = outside(&service, &thread, &full(&through)).await;
    assert_eq!(code, 400);
    let (code, body) = outside(&service, &thread, &full(&elsewhere.join("ok"))).await;
    assert_eq!(code, 200);
    assert_eq!(body["content"], "out");
    assert_eq!(body["path"], full(&elsewhere.join("real.txt")));
}

#[cfg(windows)]
#[tokio::test]
async fn windows_path_forms_reach_an_outside_file() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    let root = checkout(&service, &thread).await;
    std::fs::write(root.join("inside.txt"), "in").unwrap();
    let elsewhere = dir.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::write(elsewhere.join("Notes.txt"), "out").unwrap();
    let native = full(&elsewhere.join("Notes.txt"));
    for spelling in [
        native.clone(),
        native.replace('\\', "/"),
        native.to_lowercase(),
        format!(r"\\?\{native}"),
    ] {
        let (code, body) = outside(&service, &thread, &spelling).await;
        assert_eq!(code, 200, "{spelling}: {body}");
        assert_eq!(body["content"], "out");
    }
    // The checkout in another case is still the checkout.
    let shouted = full(&root.join("inside.txt")).to_uppercase();
    let (code, _) = outside(&service, &thread, &shouted).await;
    assert_eq!(code, 400);
    // Another machine is refused before it is contacted.
    for remote in [
        r"\\server.invalid\share\a.txt",
        r"\\?\UNC\server.invalid\share\a.txt",
    ] {
        let (code, body) = outside(&service, &thread, remote).await;
        assert_eq!(code, 400, "{remote}: {body}");
    }
}

#[tokio::test]
async fn an_untrusted_folder_shows_nothing_outside_either() {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), Options::default());
    let folder = dir.path().join("project");
    std::fs::create_dir_all(&folder).unwrap();
    let project = service.project(&folder, false).await;
    let thread = service.thread(&project, "codex").await;
    let file = dir.path().join("note.txt");
    std::fs::write(&file, "x").unwrap();
    let (code, _) = outside(&service, &thread, &full(&file)).await;
    assert_eq!(code, 403);
}

/// A real repository with one commit on `main`, outside the workspace library.
fn repository_with_file(path: &std::path::Path, file: &str, content: &str) {
    std::fs::create_dir_all(path).unwrap();
    run_git(path, &["init", "-q", "-b", "main"]);
    run_git(path, &["config", "user.email", "t@example.com"]);
    run_git(path, &["config", "user.name", "T"]);
    run_git(path, &["config", "maintenance.auto", "false"]);
    std::fs::write(path.join(file), content).unwrap();
    run_git(path, &["add", "."]);
    run_git(path, &["commit", "-q", "-m", "first"]);
}

/// A service with the multi-repository beta on and a thread in two repositories, each on
/// its own worktree of the thread's folder. Both worktrees hold `same.png` with different
/// bytes: the primary's is `primary` and the other repository's is `other`.
struct RepoThread {
    // The service stops before the folder it holds is removed: fields drop in this order.
    service: Service,
    project: String,
    other: String,
    thread: String,
    cwd: std::path::PathBuf,
    primary_root: std::path::PathBuf,
    other_root: std::path::PathBuf,
    _dir: TempDir,
}

async fn repo_thread() -> RepoThread {
    repo_thread_with(Vec::new()).await
}

/// The same fixture with extra service environment, for a route that needs something the raw
/// tests do not (a browser path, or a PATH holding a fake editor).
async fn repo_thread_with(extra_env: Vec<(String, String)>) -> RepoThread {
    let dir = TempDir::new();
    let mut env = vec![("ORBYN_MULTI_REPO".into(), "1".into())];
    env.extend(extra_env);
    let service = Service::start(
        &dir.path().join("data"),
        Options {
            env,
            ..Options::default()
        },
    );
    let main = dir.path().join("main");
    repository_with_file(&main, "same.png", "primary");
    let project = service.project(&main, true).await;
    let other = dir.path().join("other");
    repository_with_file(&other, "same.png", "other");
    let (status, row) = service
        .send(
            &format!("/api/projects/{project}/repos/add-folder"),
            json!({ "path": full(&other) }),
        )
        .await;
    assert_eq!(status, 200, "{row}");
    let other_repo = row["id"].as_str().unwrap().to_owned();
    let (status, body) = service
        .send(
            &format!("/api/projects/{project}/repos/{other_repo}/trust"),
            json!({}),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let (status, thread) = service
        .send(
            &format!("/api/projects/{project}/repo-threads"),
            json!({
                "repos": [
                    { "repo_id": project, "base_branch": "main" },
                    { "repo_id": other_repo, "base_branch": "main" },
                ],
                "engine": "codex",
                "model": null,
                "permission_mode": "full_access",
            }),
        )
        .await;
    assert_eq!(status, 200, "{thread}");
    let thread = thread["id"].as_str().unwrap().to_owned();
    let held = service.json(&format!("/api/threads/{thread}")).await;
    let cwd = std::path::PathBuf::from(held["cwd"].as_str().unwrap());
    let listed = service.json(&format!("/api/threads/{thread}/repos")).await;
    let worktree = |repo_id: &str| {
        let name = listed
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["repo_id"] == repo_id)
            .unwrap_or_else(|| panic!("no row for {repo_id}: {listed}"))["name"]
            .as_str()
            .unwrap()
            .to_owned();
        cwd.join(name)
    };
    RepoThread {
        primary_root: worktree(&project),
        other_root: worktree(&other_repo),
        service,
        project,
        other: other_repo,
        thread,
        cwd,
        _dir: dir,
    }
}

/// `GET .../files/raw` for one path, with the repository the caller names (absent is the
/// primary) and, when asked, the download flag.
async fn raw_of(
    service: &Service,
    thread: &str,
    path: &str,
    repo: Option<&str>,
    download: bool,
) -> reqwest::Response {
    let mut query = vec![("path".to_owned(), path.to_owned())];
    if let Some(repo) = repo {
        query.push(("repo_id".to_owned(), repo.to_owned()));
    }
    if download {
        query.push(("download".to_owned(), "true".to_owned()));
    }
    service
        .get(&format!("/api/threads/{thread}/files/raw"))
        .query(&query)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn raw_bytes_read_the_repository_a_multi_repo_thread_names() {
    let fixture = repo_thread().await;
    // The same name is a different file in each repository, and the answer follows repo_id.
    let second = raw_of(
        &fixture.service,
        &fixture.thread,
        "same.png",
        Some(&fixture.other),
        false,
    )
    .await;
    assert_eq!(second.status(), 200);
    assert_eq!(header(&second, "content-type"), "image/png");
    assert!(header(&second, "content-disposition").starts_with("inline"));
    assert_eq!(second.bytes().await.unwrap().as_ref(), b"other");

    // Absent, and the primary's own id, both read the primary repository.
    for repo in [None, Some(fixture.project.as_str())] {
        let primary = raw_of(&fixture.service, &fixture.thread, "same.png", repo, false).await;
        assert_eq!(primary.status(), 200, "{repo:?}");
        assert_eq!(
            primary.bytes().await.unwrap().as_ref(),
            b"primary",
            "{repo:?}"
        );
    }

    // A path that steps through the worktree's parent and back in lands inside the named
    // repository, so it reads; the guard judges where a path lands.
    let other_name = fixture
        .other_root
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let stepped = raw_of(
        &fixture.service,
        &fixture.thread,
        &format!("../{other_name}/same.png"),
        Some(&fixture.other),
        false,
    )
    .await;
    assert_eq!(stepped.status(), 200);
    assert_eq!(stepped.bytes().await.unwrap().as_ref(), b"other");

    // Download is the same bytes of the named repository as an attachment.
    let download = raw_of(
        &fixture.service,
        &fixture.thread,
        "same.png",
        Some(&fixture.other),
        true,
    )
    .await;
    assert_eq!(download.status(), 200);
    assert!(
        header(&download, "content-disposition").starts_with("attachment"),
        "{}",
        header(&download, "content-disposition")
    );
    assert_eq!(download.bytes().await.unwrap().as_ref(), b"other");

    // A repository that is not the thread's, and any repo_id on an ordinary thread, is not found.
    let unknown = raw_of(
        &fixture.service,
        &fixture.thread,
        "same.png",
        Some("no-such-repository"),
        false,
    )
    .await;
    assert_eq!(unknown.status(), 404);
    assert_eq!(unknown.json::<Value>().await.unwrap()["code"], "not_found");
    let plain = fixture.service.thread(&fixture.project, "codex").await;
    let ordinary = raw_of(
        &fixture.service,
        &plain,
        "same.png",
        Some(&fixture.project),
        false,
    )
    .await;
    assert_eq!(ordinary.status(), 404);
    assert_eq!(ordinary.json::<Value>().await.unwrap()["code"], "not_found");
    let ordinary_default = raw_of(&fixture.service, &plain, "same.png", None, false).await;
    assert_eq!(
        ordinary_default.status(),
        200,
        "only the repo_id is refused"
    );
}

#[tokio::test]
async fn raw_bytes_stay_inside_the_repository_a_multi_repo_thread_names() {
    let fixture = repo_thread().await;
    // Both of these exist, so each refusal below is the path guard and not a missing file:
    // the primary's worktree is a sibling of the named repository's, and a file sits in
    // the thread's folder beside both worktrees.
    std::fs::write(fixture.primary_root.join("same.png"), "primary").unwrap();
    std::fs::write(fixture.cwd.join("outside.png"), "beside the worktrees").unwrap();
    let primary_name = fixture
        .primary_root
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    for path in [
        "../outside.png".to_owned(),
        format!("../{primary_name}/same.png"),
        full(&fixture.cwd.join("outside.png")),
        ".git/config".to_owned(),
    ] {
        let response = raw_of(
            &fixture.service,
            &fixture.thread,
            &path,
            Some(&fixture.other),
            false,
        )
        .await;
        assert_eq!(response.status(), 400, "{path}");
    }
    // The named repository's own file still reads after the refusals.
    let own = raw_of(
        &fixture.service,
        &fixture.thread,
        "same.png",
        Some(&fixture.other),
        false,
    )
    .await;
    assert_eq!(own.status(), 200);
    assert_eq!(own.bytes().await.unwrap().as_ref(), b"other");
}

#[tokio::test]
async fn a_repo_id_opens_the_preview_in_the_named_repositorys_worktree() {
    // A browser path that does not exist: a preview reaches the browser only after its file
    // resolved, so a resolved file answers the launch's 500 instead of opening a real browser.
    let fixture = repo_thread_with(vec![(
        "ORBYN_BROWSER".to_owned(),
        std::env::temp_dir()
            .join("orbyn-no-such-browser")
            .to_string_lossy()
            .into_owned(),
    )])
    .await;
    let service = &fixture.service;
    let other = fixture.other.clone();
    let preview = format!("/api/threads/{}/previews/file", fixture.thread);
    std::fs::write(fixture.other_root.join("only.html"), "<title>only</title>").unwrap();
    std::fs::write(fixture.other_root.join("page.html"), "<title>two</title>").unwrap();
    std::fs::write(fixture.primary_root.join("base.txt"), "base\n").unwrap();

    // The primary's preview does not see the second repository's file, and a named repository
    // does: the file resolved in that worktree, so the failure after it is the browser's.
    let (code, body) = service.send(&preview, json!({ "path": "only.html" })).await;
    assert_eq!(code, 404, "{body}");
    let (code, body) = service
        .send(
            &preview,
            json!({ "path": "only.html", "repo_id": other.clone() }),
        )
        .await;
    // The path resolved, so the configured browser is what failed: it does not exist, and the
    // launch's I/O error is the service's 500 (`internal`), never the file resolver's 404.
    assert_eq!(
        code, 500,
        "the named repository's file resolved and the browser failed after it: {body}"
    );

    // The same name is each repository's own file: a file only the primary has is not found in
    // the second repository, and the second's own page resolves.
    let (code, body) = service
        .send(
            &preview,
            json!({ "path": "base.txt", "repo_id": other.clone() }),
        )
        .await;
    assert_eq!(code, 404, "{body}");
    let (code, body) = service
        .send(
            &preview,
            json!({ "path": "page.html", "repo_id": other.clone() }),
        )
        .await;
    assert_eq!(code, 500, "{body}");

    // A path that leaves the named repository is not found, even when it names the primary's file.
    let primary_name = fixture
        .primary_root
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let (code, body) = service
        .send(
            &preview,
            json!({
                "path": format!("../{primary_name}/same.png"),
                "repo_id": other,
            }),
        )
        .await;
    assert_eq!(code, 404, "{body}");

    // A repository that is not the thread's, and an ordinary thread, are 404 as everywhere else.
    let (code, body) = service
        .send(
            &preview,
            json!({ "path": "same.png", "repo_id": "no-such" }),
        )
        .await;
    assert_eq!(code, 404, "{body}");
    let plain = service.thread(&fixture.project, "codex").await;
    let (code, body) = service
        .send(
            &format!("/api/threads/{plain}/previews/file"),
            json!({ "path": "same.png", "repo_id": fixture.other }),
        )
        .await;
    assert_eq!(code, 404, "{body}");
}

#[tokio::test]
async fn a_repo_id_on_the_outside_editor_is_not_found_for_an_unknown_repository_or_an_ordinary_thread()
 {
    let fixture = repo_thread().await;
    let service = &fixture.service;
    let other = fixture.other.clone();
    let open = format!("/api/threads/{}/files/open", fixture.thread);

    // An unknown repository is not found before the editor is chosen.
    let (code, body) = service
        .send(
            &open,
            json!({ "path": "same.png", "editor": "no-such-editor", "repo_id": "no-such" }),
        )
        .await;
    assert_eq!(code, 404, "{body}");
    // An ordinary thread has one checkout and no repositories, so any repo_id is 404.
    let plain = service.thread(&fixture.project, "codex").await;
    let (code, body) = service
        .send(
            &format!("/api/threads/{plain}/files/open"),
            json!({ "path": "same.png", "editor": "no-such-editor", "repo_id": other.clone() }),
        )
        .await;
    assert_eq!(code, 404, "{body}");
    // A path that leaves the named repository is refused before the editor is chosen.
    let primary_name = fixture
        .primary_root
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let (code, body) = service
        .send(
            &open,
            json!({
                "path": format!("../{primary_name}/same.png"),
                "editor": "no-such-editor",
                "repo_id": other,
            }),
        )
        .await;
    assert_eq!(code, 400, "{body}");
}

#[cfg(unix)]
#[tokio::test]
async fn a_repo_id_opens_the_named_repositorys_file_in_the_configured_editor() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new();
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let record = dir.path().join("opened");
    let code = bin.join("code");
    std::fs::write(
        &code,
        format!("#!/bin/sh\necho \"$1\" > '{}'\n", record.display()),
    )
    .unwrap();
    std::fs::set_permissions(&code, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let fixture = repo_thread_with(vec![("PATH".to_owned(), path)]).await;
    let service = &fixture.service;
    assert!(
        service
            .json("/api/editors")
            .await
            .as_array()
            .unwrap()
            .iter()
            .any(|editor| editor["id"] == "vscode")
    );
    let open = format!("/api/threads/{}/files/open", fixture.thread);
    let other = fixture.other.clone();
    let opened_for = |path: &std::path::Path| path.to_str().unwrap().to_owned();
    let wait_for_record = || async {
        for _ in 0..6000 {
            if let Ok(seen) = std::fs::read_to_string(&record)
                && !seen.trim().is_empty()
            {
                return seen.trim().to_owned();
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        panic!("the editor never ran");
    };

    // The named repository's worktree file, not the primary's file of the same relative name.
    let (code, body) = service
        .send(
            &open,
            json!({ "path": "same.png", "editor": "vscode", "repo_id": other }),
        )
        .await;
    assert_eq!(code, 202, "{body}");
    assert_eq!(
        wait_for_record().await,
        opened_for(&fixture.other_root.join("same.png"))
    );

    // No repo_id is the primary repository, as before: the same relative path is its worktree's. The first
    // answer is cleared so the wait cannot read it instead of this one.
    std::fs::remove_file(&record).unwrap();
    let (code, body) = service
        .send(&open, json!({ "path": "same.png", "editor": "vscode" }))
        .await;
    assert_eq!(code, 202, "{body}");
    assert_eq!(
        wait_for_record().await,
        opened_for(&fixture.primary_root.join("same.png"))
    );

    // An unknown repository opens nothing.
    std::fs::remove_file(&record).unwrap();
    let (code, _) = service
        .send(
            &open,
            json!({ "path": "same.png", "editor": "vscode", "repo_id": "no-such" }),
        )
        .await;
    assert_eq!(code, 404);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(!record.exists(), "no editor ran for an unknown repository");
}
