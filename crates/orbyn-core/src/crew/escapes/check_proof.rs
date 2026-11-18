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
//! - **Confined where the machine can.** On macOS the command runs under `sandbox-exec` with a profile that denies the network, denies
//!   the core's data folder except the proof's own folder, and allows writes only to that folder, `/dev` and the temporary folders.
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
        crate::host::confine::unavailable().is_none()
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

/// Runs `run` in `dir`: `Some(true)` on exit 0, `Some(false)` on any other exit, `None` when it did not start, ran out of `limit`,
/// or was cancelled. Output is discarded. `data` is the core's data folder, which a confined run may not read.
pub async fn run(
    run: &str,
    dir: &Path,
    data: &Path,
    confined: bool,
    limit: Duration,
    cancel: &CancellationToken,
) -> Option<bool> {
    let (program, args) = crate::host::terminal::shell_command(run);
    let mut command = tokio::process::Command::new(&program);
    command.args(&args);
    if confined {
        command = sandboxed(run, dir, data)?;
    }
    command
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .env_remove("ORBYN_DATA_DIR")
        .kill_on_drop(true);
    crate::host::child_env::for_tool(command.as_std_mut());
    let mut child = crate::host::process::ManagedChild::spawn(command).ok()?;
    tokio::select! {
        biased;
        () = cancel.cancelled() => None,
        waited = tokio::time::timeout(limit, child.wait()) => match waited {
            Ok(Ok(status)) => Some(status.success()),
            _ => None,
        },
    }
}

#[cfg(target_os = "macos")]
fn sandboxed(run: &str, dir: &Path, data: &Path) -> Option<tokio::process::Command> {
    let dir = std::fs::canonicalize(dir).ok()?;
    let data = std::fs::canonicalize(data).ok()?;
    let mut profile = String::from(
        "(version 1)\n(allow default)\n(deny network*)\n\
         (deny file-write* (subpath \"/\"))\n\
         (allow file-write* (subpath \"/dev\"))\n\
         (allow file-write* (subpath \"/private/tmp\"))\n\
         (allow file-write* (subpath \"/private/var/folders\"))\n\
         (deny file-read* file-write* (subpath (param \"DATA\")))\n\
         (allow file-read* file-write* (subpath (param \"RUN\")))\n",
    );
    let mut parameters = vec![
        ("DATA".to_owned(), data.clone()),
        ("RUN".to_owned(), dir.clone()),
    ];
    for (index, folder) in dir
        .ancestors()
        .skip(1)
        .filter(|f| f.starts_with(&data))
        .enumerate()
    {
        profile.push_str(&format!(
            "(allow file-read-metadata (literal (param \"M{index}\")))\n"
        ));
        parameters.push((format!("M{index}"), folder.to_path_buf()));
    }
    profile.push_str(
        "(deny mach-priv-task-port)\n(deny appleevent-send)\n(deny lsopen)\n(deny signal)\n\
         (allow signal (target self) (target children) (target same-sandbox))\n",
    );
    let (program, args) = crate::host::terminal::shell_command(run);
    let mut command = tokio::process::Command::new("/usr/bin/sandbox-exec");
    command.arg("-p").arg(profile);
    for (name, path) in parameters {
        let mut pair = std::ffi::OsString::from(format!("{name}="));
        pair.push(path);
        command.arg("-D").arg(pair);
    }
    command.arg(program).args(args);
    Some(command)
}

#[cfg(not(target_os = "macos"))]
fn sandboxed(_: &str, _: &Path, _: &Path) -> Option<tokio::process::Command> {
    None
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
        std::fs::create_dir_all(&data).unwrap();
        let tree = data.join("tree");
        std::fs::create_dir_all(&tree).unwrap();
        let cancel = CancellationToken::new();
        let go = |cmd: &str, limit: Duration, token: &CancellationToken| {
            let (cmd, tree, data, token) =
                (cmd.to_owned(), tree.clone(), data.clone(), token.clone());
            async move { run(&cmd, &tree, &data, false, limit, &token).await }
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

    /// On a Mac that can restrict: no network and no reading of the data folder, where an unrestricted run has both.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn a_confined_run_has_no_network_and_cannot_read_the_data_folder() {
        if !confined() {
            return;
        }
        let dir = TempDir::new();
        let data = std::fs::canonicalize(dir.path()).unwrap().join("data");
        let tree = data.join("tree");
        std::fs::create_dir_all(&tree).unwrap();
        std::fs::write(data.join("secret.txt"), "secret").unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let cancel = CancellationToken::new();
        let limit = Duration::from_secs(30);
        let connect = format!("/usr/bin/nc -z 127.0.0.1 {port}");
        assert_eq!(
            run(&connect, &tree, &data, false, limit, &cancel).await,
            Some(true)
        );
        assert_eq!(
            run(&connect, &tree, &data, true, limit, &cancel).await,
            Some(false)
        );
        let read = format!("/bin/cat {}", data.join("secret.txt").display());
        assert_eq!(
            run(&read, &tree, &data, false, limit, &cancel).await,
            Some(true)
        );
        assert_eq!(
            run(&read, &tree, &data, true, limit, &cancel).await,
            Some(false)
        );
        // It can write in its own folder and nowhere else under the data folder.
        assert_eq!(
            run("/usr/bin/touch own.txt", &tree, &data, true, limit, &cancel).await,
            Some(true)
        );
        let elsewhere = format!("/usr/bin/touch {}", data.join("other.txt").display());
        assert_eq!(
            run(&elsewhere, &tree, &data, true, limit, &cancel).await,
            Some(false)
        );
        assert!(
            tree.join("own.txt").exists(),
            "own.txt missing; tree has {:?}",
            std::fs::read_dir(&tree)
                .unwrap()
                .map(|e| e.unwrap().file_name())
                .collect::<Vec<_>>()
        );
        assert!(!data.join("other.txt").exists(), "other.txt was written");
    }
}
