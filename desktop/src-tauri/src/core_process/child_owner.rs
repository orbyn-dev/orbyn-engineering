//! A child must have an exit owner before readiness can fail.

use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const STOP_GRACE: Duration = Duration::from_secs(5);

pub(super) struct StartupGuard {
    child: Arc<Mutex<Child>>,
    stopping: Arc<AtomicBool>,
    armed: bool,
}

impl StartupGuard {
    pub(super) fn new(child: Arc<Mutex<Child>>, stopping: Arc<AtomicBool>) -> Self {
        Self {
            child,
            stopping,
            armed: true,
        }
    }

    pub(super) fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for StartupGuard {
    fn drop(&mut self) {
        if self.armed {
            self.stopping.store(true, Ordering::SeqCst);
            reap_after_grace(self.child.clone());
        }
    }
}

/// The caller closes the parent pipe first. Poll without holding the child lock
/// across the grace period, then kill and reap if the service has not exited.
pub(super) fn reap_after_grace(child: Arc<Mutex<Child>>) {
    std::thread::spawn(move || {
        let deadline = Instant::now() + STOP_GRACE;
        loop {
            if let Ok(mut child) = child.lock() {
                match child.try_wait() {
                    Ok(Some(_)) => return,
                    _ if Instant::now() >= deadline => {
                        let _ = child.kill();
                        let _ = child.wait();
                        return;
                    }
                    _ => {}
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};

    #[test]
    #[ignore = "subprocess fixture"]
    fn fixture() {
        if std::env::var_os("ORBYN_STARTUP_CHILD_FIXTURE").is_some() {
            println!("fixture ready");
            std::thread::sleep(Duration::from_secs(60));
        }
    }

    /// A child that exits when its stdin closes, as the service does once the shell drops the parent pipe.
    #[test]
    #[ignore = "subprocess fixture"]
    fn fixture_exits_on_eof() {
        if std::env::var_os("ORBYN_STARTUP_CHILD_EOF_FIXTURE").is_some() {
            println!("fixture ready");
            let mut sink = String::new();
            while std::io::stdin().read_line(&mut sink).unwrap_or(0) > 0 {
                sink.clear();
            }
            // Out before the harness prints its result into a pipe the parent has already let go of.
            std::process::exit(0);
        }
    }

    fn child() -> Arc<Mutex<Child>> {
        spawn("fixture", "ORBYN_STARTUP_CHILD_FIXTURE")
    }

    fn spawn(name: &str, variable: &str) -> Arc<Mutex<Child>> {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &format!("{}::{name}", module_path!().split_once("::").unwrap().1),
                "--ignored",
                "--nocapture",
            ])
            .env(variable, "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        while !line.contains("fixture ready") {
            assert!(output.read_line(&mut line).unwrap() > 0);
        }
        Arc::new(Mutex::new(child))
    }

    #[test]
    fn failed_startup_marks_stopping_and_reaps_a_child_that_ignores_eof() {
        let child = child();
        let stopping = Arc::new(AtomicBool::new(false));
        let guard = StartupGuard::new(child.clone(), stopping.clone());
        let stdin = child.lock().unwrap().stdin.take();
        drop(stdin);
        drop(guard);
        assert!(stopping.load(Ordering::SeqCst));
        let deadline = Instant::now() + Duration::from_secs(8);
        while child.lock().unwrap().try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                let _ = child.lock().unwrap().kill();
                panic!("startup failure left its child alive");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn successful_startup_transfers_ownership_without_stopping_the_child() {
        let child = child();
        let stopping = Arc::new(AtomicBool::new(false));
        let mut guard = StartupGuard::new(child.clone(), stopping.clone());
        guard.disarm();
        drop(guard);
        assert!(!stopping.load(Ordering::SeqCst));
        let mut child = child.lock().unwrap();
        assert!(child.try_wait().unwrap().is_none());
        child.kill().unwrap();
        child.wait().unwrap();
    }

    /// A stop is a closed pipe and a reaper: a service that exits on the closed pipe is reaped at once, not after the grace,
    /// and is never killed (its exit status is its own, not a kill signal).
    #[test]
    fn a_child_that_exits_on_eof_is_reaped_well_before_the_grace_ends() {
        let child = spawn("fixture_exits_on_eof", "ORBYN_STARTUP_CHILD_EOF_FIXTURE");
        let stdin = child.lock().unwrap().stdin.take();
        drop(stdin);
        reap_after_grace(child.clone());
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.lock().unwrap().try_wait().unwrap() {
                break status;
            }
            assert!(
                started.elapsed() < STOP_GRACE / 2,
                "the child that closed on EOF was still alive halfway through the grace"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        assert!(status.success(), "it exited by itself: {status}");
    }
}
