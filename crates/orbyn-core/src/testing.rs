//! Helpers shared by unit tests.

use std::path::{Path, PathBuf};

pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> Self {
        let path = std::env::temp_dir().join(format!("orbyn-test-{}", crate::util::new_id()));
        std::fs::create_dir_all(&path).unwrap();
        // Temp folders sit behind a symlink on macOS, and the workspace refuses those.
        Self(crate::util::canonicalize(&path).unwrap())
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Runs git and panics on failure, for arranging repositories in tests.
pub fn run_git(cwd: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// A minimal valid skills library with one skill and its host links.
pub fn library(root: &Path) -> PathBuf {
    std::fs::create_dir_all(root.join("skills/how")).unwrap();
    std::fs::write(root.join("AGENTS.md"), "shared policy\n").unwrap();
    std::fs::write(root.join(".gitignore"), "repo/\n").unwrap();
    std::fs::write(
        root.join("skills/catalog.json"),
        r#"{"skills":[{"name":"how","category":"Engineering practice"},
            {"name":"social-content","category":"Social content and visuals"}]}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("skills/how/SKILL.md"),
        "---\nname: how\ndescription: \"Trace behavior\"\n---\n\n# how\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("skills/social-content")).unwrap();
    std::fs::write(
        root.join("skills/social-content/SKILL.md"),
        "---\nname: social-content\ndescription: \"Posts\"\n---\n",
    )
    .unwrap();
    for host in [".claude", ".agents"] {
        let links = root.join(host).join("skills");
        std::fs::create_dir_all(&links).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("../../skills/how", links.join("how")).unwrap();
    }
    run_git(root, &["init", "-q", "-b", "main"]);
    run_git(root, &["config", "user.email", "t@example.com"]);
    run_git(root, &["config", "user.name", "T"]);
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-q", "-m", "library"]);
    crate::util::canonicalize(root).unwrap()
}

/// A directory junction: a Windows reparse point that redirects a folder without the
/// symbolic-link privilege a symlink needs. Both paths are the caller's disposable fixture.
#[cfg(windows)]
pub fn junction(link: &Path, target: &Path) {
    // cmd.exe parses its own command line, so a forward slash in a path would read as a switch.
    let path_arg = |path: &Path| path.to_string_lossy().replace('/', "\\");
    let status = std::process::Command::new("cmd.exe")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(path_arg(link))
        .arg(path_arg(target))
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(
        status.success(),
        "could not make the junction {} -> {}",
        link.display(),
        target.display()
    );
}

/// A shell command that starts, in the background, a child which leaves its process group with `setsid`, optionally ignores SIGTERM,
/// writes its pid to `pid_file` (renamed into place once whole) and sleeps. Append what the shell does next, such as `wait`. Perl is
/// on macOS and on Linux; neither has a `setsid` program in common.
#[cfg(unix)]
pub fn setsid_sleeper(pid_file: &Path, ignore_term: bool) -> String {
    let ignore = if ignore_term {
        "$SIG{TERM}=\"IGNORE\"; "
    } else {
        ""
    };
    format!(
        "perl -e 'use POSIX; POSIX::setsid(); {ignore}open(F, \">{p}.tmp\"); print F $$; close F; rename(\"{p}.tmp\", \"{p}\"); sleep 600' &",
        p = pid_file.display()
    )
}

/// A sleeper of the same kind as [`setsid_sleeper`], but in the double-fork shape a daemon uses: `perl` forks, the middle process exits
/// at once, and the grandchild calls `setsid` and writes its pid. Its parent link is gone before any walk can read it, so only a leader
/// that adopts the orphan (`PR_SET_CHILD_SUBREAPER` on Linux) still reaches it. The shell's `wait` returns immediately here, because the
/// middle process is gone: append `sleep 600` to keep the leader alive. Linux only, because only there does a leader adopt the orphan.
#[cfg(target_os = "linux")]
pub fn double_fork_sleeper(pid_file: &Path, ignore_term: bool) -> String {
    let ignore = if ignore_term {
        "$SIG{TERM}=\"IGNORE\"; "
    } else {
        ""
    };
    format!(
        "perl -e 'use POSIX; my $middle = fork(); exit 0 if $middle; POSIX::setsid(); {ignore}open(F, \">{p}.tmp\"); print F $$; close F; rename(\"{p}.tmp\", \"{p}\"); sleep 600' &",
        p = pid_file.display()
    )
}

/// The pid the sleeper wrote, polled (no fixed delay) for up to ten seconds.
#[cfg(unix)]
pub async fn sleeper_pid(pid_file: &Path) -> i32 {
    for _ in 0..1000 {
        if let Ok(text) = std::fs::read_to_string(pid_file)
            && let Ok(pid) = text.trim().parse()
        {
            return pid;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("the escaped child never started");
}

/// The pid the sleeper wrote, polled (no fixed delay) for up to ten seconds.
#[cfg(windows)]
pub async fn sleeper_pid(pid_file: &Path) -> u32 {
    for _ in 0..1000 {
        if let Ok(text) = std::fs::read_to_string(pid_file)
            && let Ok(pid) = text.trim().parse()
        {
            return pid;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("the escaped child never started");
}

/// Whether `pid` is a live process. A killed one stays a zombie until its parent reaps it, which `ps` shows as state `Z`.
#[cfg(unix)]
pub fn process_alive(pid: i32) -> bool {
    let out = std::process::Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    let state = String::from_utf8_lossy(&out.stdout);
    !state.trim().is_empty() && !state.trim().starts_with('Z')
}

/// Polls for the process to be gone (up to ten seconds) and kills it before failing, so a failed test leaves nothing behind.
#[cfg(unix)]
pub async fn assert_process_reaped(pid: i32, how: &str) {
    for _ in 0..500 {
        if !process_alive(pid) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    // SAFETY: the test's own sleeper.
    unsafe {
        libc::kill(pid, libc::SIGKILL);
    }
    panic!("a descendant survived {how}");
}

/// Whether `pid` is a live process, from the task list Windows keeps for all users.
#[cfg(windows)]
pub fn process_alive(pid: u32) -> bool {
    let output = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\""))
}

/// Polls for the process to be gone (up to ten seconds) and kills it before failing, so a failed test leaves nothing behind.
#[cfg(windows)]
pub async fn assert_process_reaped(pid: u32, how: &str) {
    for _ in 0..500 {
        if !process_alive(pid) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    panic!("a descendant survived {how}");
}
