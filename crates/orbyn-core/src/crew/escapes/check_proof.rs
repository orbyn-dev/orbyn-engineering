//! Where and how a created check's proof runs (docs/crew/escapes.md, "As built: PR 4"). The proof runs a command the worker's test file
//! is part of, so it is never run unrestricted and unreviewed:
//!
//! - **Its own tree, with no `.git` of the person's.** The commit is written into a fresh folder under the data folder from
//!   `git archive` (read through the `tar` crate, which refuses a path that leaves the folder), and the new file is laid over it. The
//!   folder has no repository, so a test cannot write a hook or a setting into the person's repository, and a `.git` entry in the
//!   archive is refused.
//! - **No symlink is followed when laying the file.** Each folder of the new file's path is looked at with `symlink_metadata`; a link
//!   anywhere on it, or a file already there that is a link, is refused.
//! - **Output is discarded** (`/dev/null`): nothing is read into memory, so a chatty test cannot grow it. The command is stopped at the
//!   deadline or when the check is withdrawn, and its whole process tree with it.
//! - **A scrubbed environment.** Only `PATH`, a `HOME` and a `TMPDIR` inside the proof's own folder, `LANG` and `CI=1` reach the
//!   command, never a token, key or session marker of the person's.
//! - **Confined where the machine can.** On macOS the command runs under `sandbox-exec` with the profile a crew worker gets (the
//!   data folder denied, Claude's saved conversations denied, no reading of other processes' command lines or environment, no task
//!   port, Apple events, LaunchServices or signals to others), and in addition no network, no launchd job, no writing of any app's
//!   preferences, no clipboard, no reading of the person's credential folders (`~/.ssh`, `~/.aws`, `~/.gnupg`, `~/.config/gh`, `~/.netrc`,
//!   `~/.docker`, `~/.kube`, `Library/Keychains`), and file writes only inside the proof's own folder and `/dev/null`: not the shared
//!   temporary folders. This is the broad macOS sandbox (everything not named stays allowed), so it is a mitigation, not a boundary: other
//!   system services than those named stay reachable.
//!   Anywhere else no such restriction exists, so [`confined`] is false and the core does not run the proof until a person has approved
//!   the exact change (its commit and a hash over its paths and bytes).

use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use crate::error::{CoreError, Result};

/// The most an archive may write into the proof's folder.
const TREE_MAX: u64 = 1024 * 1024 * 1024;

/// Whether this machine can confine the proof (the same restriction a crew worker needs, plus no network). The first call starts a
/// harmless process, so call it off the async threads.
pub fn confined() -> bool {
    #[cfg(target_os = "macos")]
    {
        crate::host::confine::proof_unavailable().is_none()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// A SHA-256 over the new files' paths and bytes, in path order: what a person's approval binds, with the commit.
pub fn diff_hash(files: &[(String, Vec<u8>)]) -> String {
    let mut sorted: Vec<&(String, Vec<u8>)> = files.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = Sha256::new();
    for (path, bytes) in sorted {
        hasher.update(path.len().to_le_bytes());
        hasher.update(path.as_bytes());
        hasher.update(bytes.len().to_le_bytes());
        hasher.update(bytes);
    }
    crate::util::hex(&hasher.finalize())
}

/// Writes `commit` of `repo` into `dest` (made here, empty) from `git archive`. Blocking: call it off the async threads.
pub fn extract(repo: &Path, commit: &str, dest: &Path) -> Result<()> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    std::fs::create_dir_all(dest)?;
    let mut command = Command::new("git");
    command
        .current_dir(repo)
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.hooksPath=/dev/null",
            "archive",
            "--format=tar",
            commit,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    crate::host::child_env::for_tool(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| CoreError::Git(format!("Could not run git: {error}")))?;
    let stdout = child.stdout.take().expect("piped");
    let mut archive = tar::Archive::new(stdout.take(TREE_MAX));
    archive.set_preserve_permissions(false);
    archive.set_unpack_xattrs(false);
    let mut unpacked = Ok(());
    match archive.entries() {
        Ok(entries) => {
            for entry in entries {
                let mut entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        unpacked = Err(error.to_string());
                        break;
                    }
                };
                let named_git = entry.path().is_ok_and(|path| {
                    path.components().any(|c| {
                        c.as_os_str()
                            .to_str()
                            .is_none_or(|name| name.eq_ignore_ascii_case(".git"))
                    })
                });
                if named_git {
                    unpacked = Err("the archive holds a .git entry".into());
                    break;
                }
                // A link in the commit is left out: a test could read through it, and a test that needs one fails on both runs and is
                // refused, which is the safe answer.
                let kind = entry.header().entry_type();
                if kind.is_symlink() || kind.is_hard_link() {
                    continue;
                }
                // `unpack_in` refuses a path that would leave `dest`.
                match entry.unpack_in(dest) {
                    Ok(true) => {}
                    Ok(false) => {
                        unpacked = Err("the archive names a path outside its folder".into());
                        break;
                    }
                    Err(error) => {
                        unpacked = Err(error.to_string());
                        break;
                    }
                }
            }
        }
        Err(error) => unpacked = Err(error.to_string()),
    }
    if unpacked.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    match unpacked {
        Ok(()) if status.success() => Ok(()),
        Ok(()) => Err(CoreError::Git("git archive failed".into())),
        Err(why) => Err(CoreError::Git(format!(
            "the commit could not be unpacked: {why}"
        ))),
    }
}

/// Lays `files` over `root` without following a link: every folder on the way is looked at with `symlink_metadata`, a link or a
/// non-folder on the way is refused, and so is a file that is already a link.
pub fn lay(root: &Path, files: &[(String, Vec<u8>)]) -> Result<()> {
    for (path, bytes) in files {
        let mut at: PathBuf = root.to_path_buf();
        let parts: Vec<&str> = path.split('/').collect();
        for (index, part) in parts.iter().enumerate() {
            at.push(part);
            let last = index + 1 == parts.len();
            match std::fs::symlink_metadata(&at) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    return Err(CoreError::Invalid(format!(
                        "{path} goes through a link in the commit, so it was not written."
                    )));
                }
                Ok(meta) if !last && !meta.is_dir() => {
                    return Err(CoreError::Invalid(format!(
                        "{path} has a file where a folder should be."
                    )));
                }
                Ok(meta) if last && meta.is_dir() => {
                    return Err(CoreError::Invalid(format!(
                        "{path} is a folder in the commit."
                    )));
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    if !last {
                        std::fs::create_dir(&at)?;
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }
        std::fs::write(&at, bytes)?;
    }
    Ok(())
}

/// The folder of the proof's tree inside its scratch root `root`; `home` and `tmp` beside it are the command's `HOME` and `TMPDIR`.
pub fn tree(root: &Path) -> PathBuf {
    root.join("tree")
}

/// Runs `run` in the tree of `root` (made by [`extract`]): `Some(true)` on exit 0, `Some(false)` on any other exit, `None` when it
/// did not start, ran out of `limit`, or was cancelled. Output is discarded. `data` is the core's data folder, which a confined run may
/// not read; `root` is inside it and is the one place a confined run may write.
pub async fn run(
    run: &str,
    root: &Path,
    data: &Path,
    confined: bool,
    limit: Duration,
    cancel: &CancellationToken,
) -> Option<bool> {
    let command = proof_command(run, root, data, confined)?;
    let mut child = crate::host::process::ManagedChild::spawn(command).ok()?;
    tokio::select! {
        biased;
        () = cancel.cancelled() => None,
        // Recorded as it runs, so a descendant that called `setsid` and outlives a leader that exits is still reaped on the drop below.
        waited = tokio::time::timeout(limit, child.wait_tracking(Duration::from_millis(100))) => match waited {
            Ok(Ok(status)) => Some(status.success()),
            _ => None,
        },
    }
}

/// The one place a proof's process is built, so its environment is narrowed here and nowhere else: only `PATH`, `HOME` (the scratch
/// `home`), `TMPDIR` (the scratch `tmp`), `LANG` and `CI=1` (plus what Windows needs to start a program). No token, key or session
/// marker is inherited, whatever its name. `None` when it was to be confined and could not be.
fn proof_command(
    run: &str,
    root: &Path,
    data: &Path,
    confined: bool,
) -> Option<tokio::process::Command> {
    let (home, tmp) = (root.join("home"), root.join("tmp"));
    std::fs::create_dir_all(tree(root)).ok()?;
    std::fs::create_dir_all(&home).ok()?;
    std::fs::create_dir_all(&tmp).ok()?;
    let (program, args) = crate::host::terminal::shell_command(run);
    let mut resolved = crate::host::process::ResolvedCommand {
        path: program.clone(),
        program,
        prefix: args,
    };
    if confined {
        let projects = crate::agents::usage::history_root(crate::agents::usage::Engine::Claude)?;
        crate::host::confine::Confinement::new(data, root, &[], &[])
            .and_then(|rules| rules.with_closed_claude_folder(&projects))
            .and_then(|rules| rules.wrap_proof(&mut resolved))
            .ok()?;
    }
    let mut command = tokio::process::Command::new(&resolved.program);
    command
        .args(&resolved.prefix)
        .current_dir(tree(root))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let plain = command.as_std_mut();
    plain.env_clear();
    // The one gate for what a spawned program inherits; it sets `PATH`.
    crate::host::child_env::for_tool(plain);
    plain
        .env("HOME", &home)
        .env("TMPDIR", &tmp)
        .env("LANG", "en_US.UTF-8")
        .env("CI", "1");
    #[cfg(windows)]
    for name in ["SYSTEMROOT", "COMSPEC", "PATHEXT"] {
        if let Some(value) = std::env::var_os(name) {
            plain.env(name, value);
        }
    }
    Some(command)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TempDir, run_git};

    fn files(path: &str, body: &str) -> Vec<(String, Vec<u8>)> {
        vec![(path.to_owned(), body.as_bytes().to_vec())]
    }

    #[test]
    fn the_hash_binds_paths_and_bytes_and_ignores_order() {
        let a = diff_hash(&files("tests/a.txt", "x"));
        assert_eq!(a.len(), 64);
        assert_ne!(a, diff_hash(&files("tests/a.txt", "y")));
        assert_ne!(a, diff_hash(&files("tests/b.txt", "x")));
        let two = vec![("b".to_owned(), vec![1]), ("a".to_owned(), vec![2])];
        let swapped = vec![("a".to_owned(), vec![2]), ("b".to_owned(), vec![1])];
        assert_eq!(diff_hash(&two), diff_hash(&swapped));
        // A boundary shifted between a path and a body is not the same change.
        assert_ne!(diff_hash(&files("ab", "c")), diff_hash(&files("a", "bc")));
    }

    #[test]
    fn a_new_file_is_never_written_through_a_link_or_over_a_folder() {
        let dir = TempDir::new();
        let root = dir.path().join("tree");
        std::fs::create_dir_all(root.join("tests")).unwrap();
        lay(&root, &files("tests/a.txt", "ok")).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("tests/a.txt")).unwrap(),
            "ok"
        );
        lay(&root, &files("tests/deeper/b.txt", "ok")).unwrap();
        // A folder where the file goes, and a file where a folder goes.
        std::fs::create_dir_all(root.join("tests/dir")).unwrap();
        assert!(lay(&root, &files("tests/dir", "x")).is_err());
        assert!(lay(&root, &files("tests/a.txt/c.txt", "x")).is_err());
        #[cfg(unix)]
        {
            let outside = dir.path().join("outside");
            std::fs::create_dir_all(&outside).unwrap();
            std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
            assert!(lay(&root, &files("link/escape.txt", "x")).is_err());
            std::os::unix::fs::symlink(outside.join("target.txt"), root.join("tests/l.txt"))
                .unwrap();
            assert!(lay(&root, &files("tests/l.txt", "x")).is_err());
            assert!(!outside.join("escape.txt").exists() && !outside.join("target.txt").exists());
        }
    }

    #[test]
    fn the_archive_is_a_tree_with_no_repository_of_the_persons() {
        let dir = TempDir::new();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        run_git(&repo, &["init", "-q"]);
        // The fixture asserts the exact bytes extracted from the commit.
        run_git(&repo, &["config", "core.autocrlf", "false"]);
        std::fs::write(repo.join("src/a.txt"), "one\n").unwrap();
        run_git(&repo, &["add", "-A"]);
        run_git(
            &repo,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.test",
                "commit",
                "-q",
                "-m",
                "one",
            ],
        );
        let out = std::process::Command::new("git")
            .current_dir(&repo)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap();
        let commit = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        let tree = dir.path().join("tree");
        extract(&repo, &commit, &tree).unwrap();
        assert_eq!(
            std::fs::read_to_string(tree.join("src/a.txt")).unwrap(),
            "one\n"
        );
        assert!(!tree.join(".git").exists(), "the tree has no repository");
        assert!(extract(&repo, &"0".repeat(40), &dir.path().join("bad")).is_err());
    }

    #[tokio::test]
    async fn a_run_reports_the_exit_a_cancel_and_a_deadline_and_keeps_no_output() {
        let dir = TempDir::new();
        let data = dir.path().join("data");
        let root = data.join("proof");
        std::fs::create_dir_all(&data).unwrap();
        let cancel = CancellationToken::new();
        let go = |cmd: &str, limit: Duration, token: &CancellationToken| {
            let (cmd, root, data, token) =
                (cmd.to_owned(), root.clone(), data.clone(), token.clone());
            async move { run(&cmd, &root, &data, false, limit, &token).await }
        };
        let long = Duration::from_secs(30);
        assert_eq!(go("git --version", long, &cancel).await, Some(true));
        assert_eq!(
            go("git grep --no-index -q zzz -- nothing", long, &cancel).await,
            Some(false)
        );
        assert_eq!(go("git --version", Duration::ZERO, &cancel).await, None);
        let stop = CancellationToken::new();
        stop.cancel();
        assert_eq!(go("git --version", long, &stop).await, None);
    }

    /// A test that calls `setsid` leaves the process group, which the deadline and a withdraw used to kill alone: its whole tree is
    /// reaped now, and so is a `setsid` child left running by a leader that exits 0. The pid is polled for, with no fixed delay, so a
    /// slow machine only waits longer.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_run_reaps_a_setsid_descendant_at_its_deadline_on_a_cancel_and_after_the_leader_exits()
     {
        use crate::testing::{assert_process_reaped, process_alive, setsid_sleeper, sleeper_pid};
        for how in ["the deadline", "a cancel", "the leader exiting 0"] {
            let dir = TempDir::new();
            let data = dir.path().join("data");
            let root = data.join("proof");
            std::fs::create_dir_all(&data).unwrap();
            let pid_file = data.join("pid");
            let token = CancellationToken::new();
            let limit = Duration::from_secs(if how == "the deadline" { 8 } else { 600 });
            let after = if how == "the leader exiting 0" {
                format!(
                    "while [ ! -f '{}' ]; do sleep 0.05; done; sleep 1; exit 0",
                    pid_file.display()
                )
            } else {
                "wait".to_owned()
            };
            let started = tokio::spawn({
                let script = format!("{} {after}", setsid_sleeper(&pid_file, false));
                let (root, data, token) = (root.clone(), data.clone(), token.clone());
                async move { run(&script, &root, &data, false, limit, &token).await }
            });
            let escaped = sleeper_pid(&pid_file).await;
            assert!(process_alive(escaped));
            if how == "a cancel" {
                token.cancel();
            }
            let expected = if how == "the leader exiting 0" {
                Some(true)
            } else {
                None
            };
            assert_eq!(started.await.unwrap(), expected, "{how}");
            assert_process_reaped(escaped, how).await;
        }
    }

    /// A check that double-forks has no parent link left to the run by the time the deadline or a withdraw stops it, so the group kill
    /// alone let the grandchild live on. Linux adopts it under the leader, where the walk finds it; macOS keeps that limit, so the case
    /// is Linux-only. The pid is polled for, with no fixed delay.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn a_run_reaps_a_double_forked_grandchild_at_its_deadline_and_on_a_cancel() {
        use crate::testing::{
            assert_process_reaped, double_fork_sleeper, process_alive, sleeper_pid,
        };
        for how in ["the deadline", "a cancel"] {
            let dir = TempDir::new();
            let data = dir.path().join("data");
            let root = data.join("proof");
            std::fs::create_dir_all(&data).unwrap();
            let pid_file = data.join("pid");
            let token = CancellationToken::new();
            let limit = Duration::from_secs(if how == "the deadline" { 8 } else { 600 });
            let started = tokio::spawn({
                let script = format!("{} sleep 600", double_fork_sleeper(&pid_file, false));
                let (root, data, token) = (root.clone(), data.clone(), token.clone());
                async move { run(&script, &root, &data, false, limit, &token).await }
            });
            let escaped = sleeper_pid(&pid_file).await;
            assert!(process_alive(escaped));
            if how == "a cancel" {
                token.cancel();
            }
            assert_eq!(started.await.unwrap(), None, "{how}");
            assert_process_reaped(escaped, how).await;
        }
    }

    /// The command gets a short, fixed environment: its own `HOME` and `TMPDIR` inside the scratch folder, `CI=1`, and nothing that
    /// looks like a token, key or session marker of the person's.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_run_gets_only_a_scrubbed_environment_with_its_own_home_and_tmp() {
        let dir = TempDir::new();
        let data = dir.path().join("data");
        let root = data.join("proof");
        std::fs::create_dir_all(&data).unwrap();
        let cancel = CancellationToken::new();
        assert_eq!(
            run(
                "/usr/bin/env > ../env.txt",
                &root,
                &data,
                false,
                Duration::from_secs(30),
                &cancel
            )
            .await,
            Some(true)
        );
        let text = std::fs::read_to_string(root.join("env.txt")).unwrap();
        let names: Vec<&str> = text.lines().filter_map(|l| l.split('=').next()).collect();
        for name in ["PATH", "HOME", "TMPDIR", "CI"] {
            assert!(names.contains(&name), "{name} is set: {names:?}");
        }
        assert!(
            text.contains(&format!("HOME={}", root.join("home").display())),
            "{text}"
        );
        assert!(
            text.contains(&format!("TMPDIR={}", root.join("tmp").display())),
            "{text}"
        );
        for name in &names {
            let upper = name.to_ascii_uppercase();
            for secret in [
                "TOKEN",
                "SECRET",
                "KEY",
                "PASSWORD",
                "ANTHROPIC",
                "OPENAI",
                "CLAUDE",
                "ORBYN",
                "GITHUB",
                "AWS_",
                "SSH_AUTH_SOCK",
                "CODEX",
            ] {
                assert!(
                    !upper.contains(secret),
                    "{name} leaked into the proof: {names:?}"
                );
            }
        }
    }

    /// On a Mac that can restrict: everything the worker's profile denies, and in addition no network, no other process's environment,
    /// no launchd job, no preference write, no clipboard, unreadable credential folders, and file writes only in its own scratch folder (and `/dev/null`). Where an unrestricted run has the
    /// ability, the same command is run unrestricted first, so the test proves the restriction and not a broken command.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn a_confined_run_has_every_worker_rule_and_the_proofs_own() {
        if !confined() {
            return;
        }
        let dir = TempDir::new();
        let data = std::fs::canonicalize(dir.path()).unwrap().join("data");
        let root = data.join("proof");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("secret.txt"), "secret").unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let cancel = CancellationToken::new();
        let limit = Duration::from_secs(30);
        let go = |cmd: String, confined: bool| {
            let (root, data, cancel) = (root.clone(), data.clone(), cancel.clone());
            async move { run(&cmd, &root, &data, confined, limit, &cancel).await }
        };
        // No network.
        let connect = format!("/usr/bin/nc -z 127.0.0.1 {port}");
        assert_eq!(go(connect.clone(), false).await, Some(true));
        assert_eq!(go(connect, true).await, Some(false));
        // The data folder is unreadable.
        let read = format!("/bin/cat {}", data.join("secret.txt").display());
        assert_eq!(go(read.clone(), false).await, Some(true));
        assert_eq!(go(read, true).await, Some(false));
        // Another process's command line and environment are unreadable. (`ps eww` shows no environment even unrestricted on a
        // current macOS, so the positive control is the command line, which the same rule covers.)
        let other = std::process::Command::new("/bin/sh")
            .args(["-c", "sleep 60; true", "orbyn-proof-probe-hunter2"])
            .env("ORBYN_PROOF_PROBE", "hunter2")
            .spawn()
            .unwrap();
        let spy = format!(
            "/bin/ps -o command= -p {} | /usr/bin/grep -q hunter2",
            other.id()
        );
        assert_eq!(
            go(spy.clone(), false).await,
            Some(true),
            "the probe works unrestricted"
        );
        assert_eq!(go(spy, true).await, Some(false));
        let spy_env = format!("/bin/ps eww -p {} | /usr/bin/grep -q hunter2", other.id());
        assert_eq!(go(spy_env, true).await, Some(false));
        let mut other = other;
        let _ = other.kill();
        let _ = other.wait();
        // Writes: not the shared temporary folder, not the data folder, not the person's home; yes its own folders and /dev/null.
        let shared = std::path::PathBuf::from(format!(
            "/private/tmp/orbyn-proof-{}",
            crate::util::new_id()
        ));
        assert_eq!(
            go(format!("/usr/bin/touch {}", shared.display()), true).await,
            Some(false)
        );
        assert!(!shared.exists(), "a write to /private/tmp landed");
        let elsewhere = data.join("other.txt");
        assert_eq!(
            go(format!("/usr/bin/touch {}", elsewhere.display()), true).await,
            Some(false)
        );
        assert!(!elsewhere.exists());
        assert_eq!(go("/usr/bin/touch own.txt".into(), true).await, Some(true));
        assert!(root.join("tree/own.txt").exists());
        assert_eq!(
            go("/usr/bin/touch \"$TMPDIR/own.txt\"".into(), true).await,
            Some(true)
        );
        assert!(root.join("tmp/own.txt").exists());
        assert_eq!(go("echo x > /dev/null".into(), true).await, Some(true));
        // No launchd job: it would run a program from launchd, outside this profile.
        let marker = data.join("launched.txt");
        let label = format!("orbyn.proof.{}", crate::util::new_id());
        let submit = format!(
            "/bin/launchctl submit -l {label} -- /usr/bin/touch {}",
            marker.display()
        );
        assert_eq!(go(submit, true).await, Some(false));
        let _ = std::process::Command::new("/bin/launchctl")
            .args(["remove", &label])
            .output();
        assert!(!marker.exists(), "a launchd job ran");
        // No writing of an application's preferences (`defaults write` goes through cfprefsd; a changed setting of another app can later
        // start a program outside the sandbox). The unrestricted run is the positive control; it removes what it wrote.
        let domain = format!("dev.orbyn.proofprobe{}", crate::util::new_id());
        let write = format!("/usr/bin/defaults write {domain} probe -int 1");
        assert_eq!(go(write.clone(), true).await, Some(false));
        assert_eq!(
            go(format!("/usr/bin/defaults read {domain}"), false).await,
            Some(false),
            "nothing was written"
        );
        assert_eq!(
            go(write, false).await,
            Some(true),
            "the probe works unrestricted"
        );
        let _ = std::process::Command::new("/usr/bin/defaults")
            .args(["delete", &domain])
            .output();
        // The person's credential folders are unreadable; the toolchain folders are not.
        if let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) {
            for secret in [
                ".ssh",
                ".aws",
                ".gnupg",
                ".config/gh",
                ".netrc",
                ".docker",
                ".kube",
                "Library/Keychains",
            ] {
                let path = home.join(secret);
                if path.exists() {
                    let list = format!("/bin/ls {}", path.display());
                    assert_eq!(
                        go(list.clone(), false).await,
                        Some(true),
                        "{secret}: the probe works unrestricted"
                    );
                    assert_eq!(
                        go(list, true).await,
                        Some(false),
                        "{secret} must be unreadable"
                    );
                }
            }
            for toolchain in [".cargo", ".rustup"] {
                let path = home.join(toolchain);
                if path.exists() {
                    assert_eq!(
                        go(format!("/bin/ls {}", path.display()), true).await,
                        Some(true),
                        "{toolchain} stays readable"
                    );
                }
            }
        }
        // No clipboard.
        assert_eq!(
            go("echo orbyn | /usr/bin/pbcopy".into(), true).await,
            Some(false)
        );
        // Claude's saved conversations are closed, as for a worker.
        if let Some(projects) =
            crate::agents::usage::history_root(crate::agents::usage::Engine::Claude)
            && projects.exists()
        {
            assert_eq!(
                go(format!("/bin/ls {}", projects.display()), true).await,
                Some(false)
            );
        }
    }
}
