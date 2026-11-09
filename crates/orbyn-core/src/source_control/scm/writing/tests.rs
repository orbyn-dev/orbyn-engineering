use crate::error::CoreError;

use super::*;
use crate::testing::{TempDir, run_git};

#[test]
fn structured_replies_are_validated_before_effects() {
    assert_eq!(
        commit_reply(r#"{"subject":"fix: preserve unicode é","body":"Why"}"#).unwrap(),
        "fix: preserve unicode é\n\nWhy"
    );
    for value in [
        "",
        "```json\n{}\n```",
        "Here it is:\n```json\n{\"subject\":\"fix: x\",\"body\":\"\"}\n```",
        "```json\n{\"subject\":\"fix: x\",\"body\":\"\"}\n```\n```json\n{}\n```",
        "```yaml\nsubject: fix\n```",
        r#"{"subject":"bad\nsubject","body":""}"#,
        r#"{"subject":"","body":"x"}"#,
    ] {
        assert!(commit_reply(value).is_err());
    }
    assert!(pull_reply(r#"{"title":"Valid","body":""}"#).is_err());
    // Claude's usual reply: one fenced block, tagged or not, with blank lines around.
    assert_eq!(
        commit_reply("```json\n{\"subject\":\"fix: keep fences\",\"body\":\"Why\"}\n```\n")
            .unwrap(),
        "fix: keep fences\n\nWhy"
    );
    assert_eq!(
        commit_reply("\n```\n{\"subject\":\"fix: bare fence\",\"body\":\"\"}\n```").unwrap(),
        "fix: bare fence"
    );
    // A code block inside the description is part of the JSON string.
    let pull = pull_reply(
        "```JSON\n{\"title\":\"Add fences\",\"body\":\"## Summary\\n\\n```rust\\nfn x() {}\\n```\"}\n```",
    )
    .unwrap();
    assert_eq!(pull.title, "Add fences");
    assert_eq!(pull.body, "## Summary\n\n```rust\nfn x() {}\n```");
    assert!(commit_reply(&"x".repeat(REPLY_BYTES + 1)).is_err());
}

#[test]
fn generation_needs_git_that_reads_attributes_from_a_tree() {
    for (output, version) in [
        ("git version 2.39.3 (Apple Git-146)\n", Some((2, 39))),
        ("git version 2.40.0\n", Some((2, 40))),
        ("git version 2.45.1.windows.1\r\n", Some((2, 45))),
        ("git version 3.0.0", Some((3, 0))),
        ("not git", None),
    ] {
        assert_eq!(git_version(output), version, "{output}");
    }
    assert!(git_version("git version 2.39.9").unwrap() < (2, 40));
}

#[test]
fn only_a_busy_changed_or_turned_off_thread_stops_the_action() {
    for error in [
        CoreError::ThreadBusy,
        CoreError::ThreadArchived,
        CoreError::FolderUntrusted,
        invalid(THREAD_CHANGED),
        invalid("claude is turned off for this project."),
        invalid("claude is turned off."),
    ] {
        assert!(stops_the_action(&error), "{error}");
    }
    for error in [
        invalid("Choose a text generation or thread model, or enter text manually."),
        invalid("This change is too large to generate text. Enter text manually."),
        invalid("Reading this change timed out. Enter text manually."),
        invalid("A writing reference is a symlink. Enter text manually."),
        invalid("The engine returned invalid commit text. Enter text manually."),
        CoreError::EngineUnavailable("claude is not installed".into()),
        CoreError::Git("fatal: bad object".into()),
    ] {
        assert!(!stops_the_action(&error), "{error}");
    }
}

#[tokio::test]
async fn candidate_reads_untracked_and_tracked_changes_without_staging() {
    let dir = TempDir::new();
    let cwd = dir.path();
    run_git(cwd, &["init", "-q", "-b", "main"]);
    run_git(cwd, &["config", "user.name", "Test"]);
    run_git(cwd, &["config", "user.email", "test@example.com"]);
    std::fs::write(cwd.join("one.txt"), "before\n").unwrap();
    run_git(cwd, &["add", "."]);
    run_git(cwd, &["commit", "-qm", "initial"]);
    std::fs::write(cwd.join("one.txt"), "after\n").unwrap();
    std::fs::write(cwd.join("new space.txt"), "new é\n").unwrap();
    let before = candidate(cwd).await.unwrap();
    assert!(before.context.contains("+after"));
    assert!(before.context.contains("new é"));
    assert!(
        read_git(cwd, &["diff", "--cached", "--name-only"])
            .await
            .unwrap()
            .is_empty()
    );
    run_git(cwd, &["add", "-A"]);
    let staged = candidate(cwd).await.unwrap();
    assert_eq!(before.identity, staged.identity);
    std::fs::write(cwd.join("one.txt"), "newer\n").unwrap();
    assert_ne!(staged.identity, candidate(cwd).await.unwrap().identity);
}

#[tokio::test]
async fn diff_reads_disable_clean_filters_and_refuse_local_info_attributes() {
    let dir = TempDir::new();
    let cwd = dir.path();
    run_git(cwd, &["init", "-q", "-b", "main"]);
    run_git(cwd, &["config", "user.name", "Test"]);
    run_git(cwd, &["config", "user.email", "test@example.com"]);
    std::fs::write(cwd.join("one.txt"), "before\n").unwrap();
    run_git(cwd, &["add", "."]);
    run_git(cwd, &["commit", "-qm", "initial"]);
    std::fs::write(cwd.join("one.txt"), "after\n").unwrap();
    std::fs::write(cwd.join(".gitattributes"), "*.txt filter=unavailable\n").unwrap();
    run_git(
        cwd,
        &[
            "config",
            "filter.unavailable.clean",
            "orbyn-nonexistent-filter-fixture",
        ],
    );
    run_git(cwd, &["config", "filter.unavailable.required", "true"]);
    assert!(candidate(cwd).await.unwrap().context.contains("+after"));
    std::fs::write(
        cwd.join(".git/info/attributes"),
        "*.txt filter=unavailable\n",
    )
    .unwrap();
    assert!(candidate(cwd).await.is_err());
}

#[tokio::test]
async fn only_a_missing_symbolic_target_is_an_unborn_head() {
    let dir = TempDir::new();
    let cwd = dir.path();
    run_git(cwd, &["init", "-q", "-b", "main"]);
    assert_eq!(head(cwd).await.unwrap(), None);
    std::fs::write(cwd.join("new.txt"), "new content\n").unwrap();
    let before = candidate(cwd).await.unwrap();
    run_git(cwd, &["add", "-A"]);
    assert_eq!(before.identity, candidate(cwd).await.unwrap().identity);
    std::fs::write(cwd.join(".git/refs/heads/main"), "malformed reference\n").unwrap();
    assert!(head(cwd).await.is_err());
    assert!(candidate(cwd).await.is_err());
}

#[tokio::test]
async fn git_effective_modes_are_part_of_the_candidate_on_every_platform() {
    let dir = TempDir::new();
    let cwd = dir.path();
    run_git(cwd, &["init", "-q", "-b", "main"]);
    run_git(cwd, &["config", "user.name", "Test"]);
    run_git(cwd, &["config", "user.email", "test@example.com"]);
    run_git(cwd, &["config", "core.filemode", "false"]);
    std::fs::write(cwd.join("one.txt"), "before\n").unwrap();
    run_git(cwd, &["add", "."]);
    run_git(cwd, &["commit", "-qm", "initial"]);
    std::fs::write(cwd.join("one.txt"), "pending content\n").unwrap();
    let before = candidate(cwd).await.unwrap();
    run_git(cwd, &["update-index", "--chmod=+x", "one.txt"]);
    let changed = candidate(cwd).await.unwrap();
    assert_ne!(before.identity, changed.identity);
    assert!(changed.context.contains("mode 100755"));
    run_git(cwd, &["update-index", "--chmod=-x", "one.txt"]);
    assert_eq!(before.identity, candidate(cwd).await.unwrap().identity);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(cwd.join("one.txt"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        // Git deliberately ignores native execute bits in this control.
        assert_eq!(before.identity, candidate(cwd).await.unwrap().identity);
        run_git(cwd, &["config", "core.filemode", "true"]);
        assert_ne!(before.identity, candidate(cwd).await.unwrap().identity);
    }
}

#[tokio::test]
async fn raw_names_preserve_deletions_renames_untracked_and_ignored_controls() {
    let dir = TempDir::new();
    let cwd = dir.path();
    run_git(cwd, &["init", "-q", "-b", "main"]);
    run_git(cwd, &["config", "user.name", "Test"]);
    run_git(cwd, &["config", "user.email", "test@example.com"]);
    for (name, content) in [
        (".gitignore", "*.tmp\n"),
        ("deleted.txt", "delete me\n"),
        ("old name.txt", "rename me\n"),
        ("tracked.tmp", "tracked despite ignore\n"),
    ] {
        std::fs::write(cwd.join(name), content).unwrap();
    }
    run_git(cwd, &["add", "-A"]);
    run_git(cwd, &["add", "-f", "tracked.tmp"]);
    run_git(cwd, &["commit", "-qm", "initial"]);
    std::fs::remove_file(cwd.join("deleted.txt")).unwrap();
    std::fs::rename(cwd.join("old name.txt"), cwd.join("new name.txt")).unwrap();
    std::fs::write(cwd.join("new untracked.txt"), "new é content\n").unwrap();
    std::fs::write(cwd.join("tracked.tmp"), "tracked changed\n").unwrap();
    std::fs::write(cwd.join("ignored.tmp"), "ignored outside candidate\n").unwrap();
    let before = candidate(cwd).await.unwrap();
    for expected in [
        "deleted.txt",
        "old name.txt",
        "new name.txt",
        "new é content",
        "tracked changed",
    ] {
        assert!(before.context.contains(expected), "{expected}");
    }
    assert!(!before.context.contains("ignored outside candidate"));
    std::fs::write(cwd.join("ignored.tmp"), "changed but still ignored\n").unwrap();
    assert_eq!(before.identity, candidate(cwd).await.unwrap().identity);
    std::fs::write(cwd.join("new untracked.txt"), "newer user data\n").unwrap();
    assert_ne!(before.identity, candidate(cwd).await.unwrap().identity);
    std::fs::write(cwd.join("new untracked.txt"), "new é content\n").unwrap();
    run_git(cwd, &["add", "-A"]);
    assert_eq!(before.identity, candidate(cwd).await.unwrap().identity);
}

#[tokio::test]
async fn detached_head_is_a_stable_candidate_marker_not_an_error() {
    let dir = TempDir::new();
    let cwd = dir.path();
    run_git(cwd, &["init", "-q", "-b", "main"]);
    run_git(cwd, &["config", "user.name", "Test"]);
    run_git(cwd, &["config", "user.email", "test@example.com"]);
    std::fs::write(cwd.join("one.txt"), "before\n").unwrap();
    run_git(cwd, &["add", "."]);
    run_git(cwd, &["commit", "-qm", "initial"]);
    std::fs::write(cwd.join("one.txt"), "after\n").unwrap();
    let attached = candidate(cwd).await.unwrap().identity;
    run_git(cwd, &["checkout", "-q", "--detach"]);
    let first = candidate(cwd).await.unwrap();
    assert_eq!(first.identity, candidate(cwd).await.unwrap().identity);
    assert_eq!(symbolic_head(cwd).await.unwrap(), b"detached HEAD");
    assert_ne!(attached, first.identity);
}

#[tokio::test]
async fn base_prefers_the_upstream_tracking_ref_and_falls_back_to_local() {
    let dir = TempDir::new();
    let cwd = dir.path();
    run_git(cwd, &["init", "-q", "-b", "main"]);
    run_git(cwd, &["config", "user.name", "Test"]);
    run_git(cwd, &["config", "user.email", "test@example.com"]);
    std::fs::write(cwd.join("one.txt"), "one\n").unwrap();
    run_git(cwd, &["add", "."]);
    run_git(cwd, &["commit", "-qm", "initial"]);
    assert_eq!(base_spec(cwd, "main").await.unwrap(), "main^{commit}");
    std::fs::write(cwd.join("one.txt"), "two\n").unwrap();
    run_git(cwd, &["commit", "-qam", "newer"]);
    run_git(cwd, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    assert_eq!(
        base_spec(cwd, "main").await.unwrap(),
        "refs/remotes/origin/main^{commit}"
    );
    run_git(
        cwd,
        &["remote", "add", "fork", "https://example.invalid/x.git"],
    );
    run_git(cwd, &["config", "branch.main.remote", "fork"]);
    assert_eq!(base_spec(cwd, "main").await.unwrap(), "main^{commit}");
    run_git(cwd, &["update-ref", "refs/remotes/fork/main", "HEAD"]);
    assert_eq!(
        base_spec(cwd, "main").await.unwrap(),
        "refs/remotes/fork/main^{commit}"
    );
}
