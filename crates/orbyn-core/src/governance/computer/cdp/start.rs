//! Starting the browser and waiting for its DevTools endpoint.

use super::super::browser_stderr::BrowserStderr;
use super::{failed, launch_flags};
use crate::error::{CoreError, Result};
use crate::host::process::ManagedChild;
use std::ffi::OsString;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

const START_TIMEOUT: Duration = Duration::from_secs(20);
/// A busy machine can start the browser slower than one bounded wait allows. A browser
/// still running when that wait ends is slow, not broken, so it gets one more bounded
/// wait; one that exited is a real failure and is reported at once.
const START_WAITS: u32 = 2;
/// A browser that died by a fault before it was ready is started once more: the crash is
/// the browser's, not the tab's, and one death should not lose the page. A browser that
/// refuses to run repeats its refusal, so that one is reported at once.
const START_ATTEMPTS: u32 = 2;

/// A browser that is running and has published its debugging endpoint.
pub(super) struct Started {
    pub(super) child: ManagedChild,
    pub(super) stderr: BrowserStderr,
    pub(super) url: String,
}

/// Starts the browser and waits for its endpoint, starting it once more when the first
/// process dies by a fault. The dead attempt's own words are logged before the restart,
/// so a crash still leaves evidence when the second start works.
pub(super) async fn start(
    path: &Path,
    profile: &Path,
    headless: bool,
    in_process_gpu: bool,
) -> Result<Started> {
    std::fs::create_dir_all(profile)?;
    let port_file = profile.join("DevToolsActivePort");
    let flags = launch_flags(path, headless, in_process_gpu);
    let mut attempt = 1;
    loop {
        // A browser that died may have written the endpoint before it went.
        let _ = std::fs::remove_file(&port_file);
        let mut child = ManagedChild::spawn(command(path, profile, &flags))?;
        let pipe = child
            .child_mut()
            .stderr
            .take()
            .ok_or_else(|| failed("browser stderr is missing"))?;
        let mut stderr = BrowserStderr::start(pipe);
        match wait_until_ready(&mut child, &port_file, &mut stderr, &flags).await {
            Ok(url) => return Ok(Started { child, stderr, url }),
            Err(failure) if failure.crashed && attempt < START_ATTEMPTS => {
                attempt += 1;
                tracing::warn!(
                    attempt,
                    error = %failure.error,
                    "the browser died before it was ready; starting it again"
                );
            }
            Err(failure) => return Err(failure.error),
        }
    }
}

/// The browser's command line. `about:blank` gives it a page from its first moment.
fn command(path: &Path, profile: &Path, flags: &[&str]) -> Command {
    let mut profile_arg = OsString::from("--user-data-dir=");
    profile_arg.push(profile);
    let mut command = Command::new(path);
    command
        .arg("--remote-debugging-port=0")
        .arg(profile_arg)
        .args(["--no-first-run", "--no-default-browser-check"])
        .args(flags)
        .arg("about:blank")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    crate::host::child_env::for_tool(command.as_std_mut());
    command
}

/// How a start failed, and whether the browser process died by a fault doing so.
struct StartFailure {
    error: CoreError,
    crashed: bool,
}

/// Waits for the browser to write its debugging endpoint, and gives a live browser that
/// is slow one more bounded wait. A browser that exited fails at once with its stderr and
/// the startup flags, and one that never writes the endpoint fails after the second wait,
/// so a real startup failure is never reported as merely slow.
async fn wait_until_ready(
    child: &mut ManagedChild,
    port_file: &Path,
    stderr: &mut BrowserStderr,
    flags: &[&str],
) -> std::result::Result<String, StartFailure> {
    let mut attempt = 1;
    loop {
        match wait_for_port(child, port_file, START_TIMEOUT).await {
            StartWait::Opened(url) => return Ok(url),
            // Slow is not failed: the live browser gets a second bounded wait.
            StartWait::TimedOut if attempt < START_WAITS => {
                attempt += 1;
                tracing::warn!(
                    attempt,
                    "the browser has not opened its debugging port yet; waiting again"
                );
            }
            StartWait::TimedOut => {
                let detail = stderr.failure_text().await;
                let error = failed("the browser did not open its debugging port");
                return Err(StartFailure {
                    error: CoreError::Computer(format!(
                        "{error}; startup flags: {flags:?}; stderr: {detail}"
                    )),
                    crashed: false,
                });
            }
            StartWait::Exited { error, crashed } => {
                let detail = stderr.failure_text().await;
                return Err(StartFailure {
                    error: CoreError::Computer(format!(
                        "{error}; startup flags: {flags:?}; stderr: {detail}"
                    )),
                    crashed,
                });
            }
        }
    }
}

async fn read_port_file(path: &Path) -> String {
    loop {
        if let Ok(text) = std::fs::read_to_string(path) {
            let mut lines = text.lines();
            if let (Some(port), Some(endpoint)) = (lines.next(), lines.next())
                && !port.is_empty()
            {
                return format!("ws://127.0.0.1:{port}{endpoint}");
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// What one bounded wait for the debugging port found.
enum StartWait {
    /// The browser wrote its endpoint; the URL is ready to connect to.
    Opened(String),
    /// The browser is still running but has not written the port yet.
    TimedOut,
    /// The browser stopped before it could be used. `crashed` says it died by a fault
    /// rather than exiting with a code it chose.
    Exited { error: CoreError, crashed: bool },
}

async fn wait_for_port(child: &mut ManagedChild, path: &Path, limit: Duration) -> StartWait {
    let waited = tokio::time::timeout(limit, async {
        tokio::select! {
            biased;
            status = child.wait() => StartWait::Exited {
                crashed: status.as_ref().is_ok_and(died_by_fault),
                error: match status {
                    Ok(status) => failed(format!("the browser exited before opening its debugging port: {status}")),
                    Err(error) => failed(format!("the browser exited before opening its debugging port: {error}")),
                },
            },
            url = read_port_file(path) => StartWait::Opened(url),
        }
    })
    .await;
    match waited {
        Ok(outcome) => outcome,
        Err(_) => StartWait::TimedOut,
    }
}

/// Whether the browser process died by a fault instead of exiting with a code it chose.
/// Only a death is worth starting again; a refusal happens again the same way.
#[cfg(unix)]
fn died_by_fault(status: &std::process::ExitStatus) -> bool {
    use std::os::unix::process::ExitStatusExt;
    // The signals a browser dies by when it crashes. A browser killed on purpose
    // (SIGTERM, SIGKILL from a stop or a shutdown) is not started again, so a stopped
    // service cannot leave a fresh browser behind.
    matches!(
        status.signal(),
        Some(
            libc::SIGILL
                | libc::SIGTRAP
                | libc::SIGABRT
                | libc::SIGBUS
                | libc::SIGFPE
                | libc::SIGSEGV
        )
    )
}

/// The same on Windows, where a fault leaves an NTSTATUS code: an access violation (its
/// `SIGSEGV`), an illegal instruction, a stack overflow, a heap fault or a fast fail.
#[cfg(windows)]
fn died_by_fault(status: &std::process::ExitStatus) -> bool {
    matches!(
        status.code().map(|code| code as u32),
        Some(0xC000_0005 | 0xC000_001D | 0xC000_00FD | 0xC000_0374 | 0xC000_0409)
    )
}

#[cfg(test)]
mod tests {
    use super::{START_TIMEOUT, StartWait, died_by_fault, wait_for_port};
    use crate::host::process::ManagedChild;
    use std::path::Path;
    use std::process::Stdio;
    use std::time::Duration;
    use tokio::process::Command;

    #[tokio::test]
    async fn browser_exit_during_startup_is_reported_without_waiting_for_timeout() {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .arg("--list")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(unix)]
        command.process_group(0);
        let mut child = ManagedChild::spawn(command).unwrap();
        let path = std::env::temp_dir().join(format!("missing-port-{}", crate::util::new_id()));
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            wait_for_port(&mut child, &path, START_TIMEOUT),
        )
        .await
        .unwrap();
        match result {
            // A test runner that refuses the browser's flags exited; it did not die.
            StartWait::Exited { error, crashed } => {
                assert!(
                    error
                        .to_string()
                        .contains("exited before opening its debugging port"),
                    "{error}"
                );
                assert!(!crashed, "an exit code is a refusal, not a death");
            }
            _ => panic!("a browser that exited is a failure, not a slow start"),
        }
    }

    /// A browser that is slow to start is waited on again rather than reported broken.
    #[cfg(any(unix, windows))]
    #[tokio::test]
    async fn a_slow_browser_gets_a_second_bounded_wait() {
        let path = std::env::temp_dir().join(format!("slow-port-{}", crate::util::new_id()));
        let mut child = slow_browser(&path).await;
        let first = wait_for_port(&mut child, &path, Duration::from_millis(300)).await;
        assert!(
            matches!(first, StartWait::TimedOut),
            "a live browser without its port is a slow start, not a failure"
        );
        let second = wait_for_port(&mut child, &path, Duration::from_secs(5)).await;
        match second {
            StartWait::Opened(url) => {
                assert_eq!(url, "ws://127.0.0.1:1234/devtools/browser/fixture")
            }
            _ => panic!("the second bounded wait must read the endpoint the browser wrote"),
        }
        drop(child);
        let _ = std::fs::remove_file(&path);
    }

    /// A browser that dies by a fault before its endpoint is up is started again, and the
    /// page still opens: that death is the browser's, not the caller's.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_browser_that_dies_before_it_is_ready_is_started_again() {
        let dir = crash_once_browser();
        let started = super::start(&dir.join("browser"), &dir.join("profile"), true, false)
            .await
            .unwrap_or_else(|error| panic!("the second start should be ready: {error}"));
        assert_eq!(started.url, "ws://127.0.0.1:1234/devtools/browser/fixture");
        assert!(
            dir.join("started").exists(),
            "the browser was started once before the restart"
        );
        // Dropping the session ends the long-lived browser the script left running.
        drop(started);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A browser that dies every time is reported after one restart, not looped on.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_browser_that_keeps_dying_is_reported_after_one_restart() {
        let dir =
            std::env::temp_dir().join(format!("browser-always-dies-{}", crate::util::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let count = dir.join("starts");
        let script = dir.join("browser");
        write_script(
            &script,
            &format!(
                "#!/bin/sh\nprintf 'x' >> '{count}'\nkill -SEGV $$\n",
                count = count.display()
            ),
        );
        let error = super::start(&script, &dir.join("profile"), true, false)
            .await
            .err()
            .expect("a browser that always dies cannot open a page");
        let text = error.to_string();
        assert!(
            text.contains("exited before opening its debugging port"),
            "{text}"
        );
        assert!(
            text.contains("startup flags:"),
            "the failure still names the flags: {text}"
        );
        let starts = std::fs::read_to_string(&count).unwrap_or_default();
        assert_eq!(starts.len(), 2, "one restart, then the failure is reported");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A death and a refusal are told apart, which is what decides the restart.
    #[cfg(unix)]
    #[test]
    fn a_fault_exit_is_told_apart_from_a_refusal() {
        use std::os::unix::process::ExitStatusExt;
        assert!(died_by_fault(&std::process::ExitStatus::from_raw(
            libc::SIGSEGV
        )));
        assert!(!died_by_fault(&std::process::ExitStatus::from_raw(1 << 8)));
        // A browser stopped on purpose is not a crash, so it is not started again.
        assert!(!died_by_fault(&std::process::ExitStatus::from_raw(
            libc::SIGTERM
        )));
        assert!(!died_by_fault(&std::process::ExitStatus::from_raw(
            libc::SIGKILL
        )));
    }

    #[cfg(windows)]
    #[test]
    fn a_fault_exit_is_told_apart_from_a_refusal() {
        use std::os::windows::process::ExitStatusExt;
        // An access violation must read as a death, an exit code as a refusal.
        assert!(died_by_fault(&std::process::ExitStatus::from_raw(
            0xC000_0005
        )));
        assert!(!died_by_fault(&std::process::ExitStatus::from_raw(1)));
    }

    /// The process a slow-start test needs: alive, and it writes the port after a delay
    /// long enough for the first wait to end. It exits long after the test does.
    #[cfg(unix)]
    async fn slow_browser(path: &Path) -> ManagedChild {
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg(format!(
                "sleep 1; printf '1234\\n/devtools/browser/fixture\\n' > '{}'; sleep 30",
                path.display()
            ))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        command.process_group(0);
        ManagedChild::spawn(command).unwrap()
    }

    #[cfg(windows)]
    async fn slow_browser(path: &Path) -> ManagedChild {
        let mut command = Command::new("cmd");
        command
            .args([
                "/C",
                &format!(
                    "ping -n 3 127.0.0.1 >NUL & echo 1234>\"{0}\" & echo /devtools/browser/fixture>>\"{0}\" & ping -n 30 127.0.0.1 >NUL",
                    path.display()
                ),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        ManagedChild::spawn(command).unwrap()
    }

    /// A folder holding a browser that dies by `SIGSEGV` on its first start and serves its
    /// endpoint on every start after that.
    #[cfg(unix)]
    fn crash_once_browser() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("browser-crash-{}", crate::util::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("browser");
        write_script(
            &script,
            &format!(
                "#!/bin/sh\n\
                 started='{marker}'\n\
                 if [ ! -e \"$started\" ]; then\n\
                 \t: > \"$started\"\n\
                 \tkill -SEGV $$\n\
                 fi\n\
                 printf '1234\\n/devtools/browser/fixture\\n' > '{port}'\n\
                 sleep 30\n",
                marker = dir.join("started").display(),
                port = dir.join("profile").join("DevToolsActivePort").display()
            ),
        );
        dir
    }

    #[cfg(unix)]
    fn write_script(path: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(path, body).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}
