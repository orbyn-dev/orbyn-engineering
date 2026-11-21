//! Waiting for a started browser to publish its DevTools endpoint.

use super::super::browser_stderr::BrowserStderr;
use super::failed;
use crate::error::{CoreError, Result};
use crate::host::process::ManagedChild;
use std::path::Path;
use std::time::Duration;

const START_TIMEOUT: Duration = Duration::from_secs(20);
/// A busy machine can start the browser slower than one bounded wait allows. A browser
/// still running when that wait ends is slow, not broken, so it gets one more bounded
/// wait; one that exited is a real failure and is reported at once.
const START_WAITS: u32 = 2;

/// Waits for the browser to write its debugging endpoint, and gives a live browser that
/// is slow one more bounded wait. A browser that exited fails at once with its stderr
/// and the startup flags, and one that never writes the endpoint fails after the second
/// wait, so a real startup failure is never reported as merely slow.
pub(super) async fn wait_until_ready(
    child: &mut ManagedChild,
    port_file: &Path,
    stderr: &mut BrowserStderr,
    flags: &[&str],
) -> Result<String> {
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
                return Err(CoreError::Computer(format!(
                    "{error}; startup flags: {flags:?}; stderr: {detail}"
                )));
            }
            StartWait::Exited(error) => {
                let detail = stderr.failure_text().await;
                return Err(CoreError::Computer(format!(
                    "{error}; startup flags: {flags:?}; stderr: {detail}"
                )));
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
    /// The browser stopped before it could be used.
    Exited(CoreError),
}

async fn wait_for_port(child: &mut ManagedChild, path: &Path, limit: Duration) -> StartWait {
    let waited = tokio::time::timeout(limit, async {
        tokio::select! {
            biased;
            status = child.wait() => StartWait::Exited(match status {
                Ok(status) => failed(format!("the browser exited before opening its debugging port: {status}")),
                Err(error) => failed(format!("the browser exited before opening its debugging port: {error}")),
            }),
            url = read_port_file(path) => StartWait::Opened(url),
        }
    })
    .await;
    match waited {
        Ok(outcome) => outcome,
        Err(_) => StartWait::TimedOut,
    }
}

#[cfg(test)]
mod tests {
    use super::{START_TIMEOUT, StartWait, wait_for_port};
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
            StartWait::Exited(error) => assert!(
                error
                    .to_string()
                    .contains("exited before opening its debugging port"),
                "{error}"
            ),
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

    /// The browser process a slow-start test needs: alive, and it writes the port after a
    /// delay long enough for the first wait to end. It exits long after the test does.
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
}
