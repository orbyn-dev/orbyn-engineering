//! Unix: the descendants of a process group leader, found by walking parent links. A process group does not hold a child that calls
//! `setsid` or moves itself to another group, so the owner of a tree also walks `ps` (the same tool on macOS and Linux) from the
//! leader and signals every descendant it finds. Only a descendant whose parent chain is still alive can be found: a daemon already
//! orphaned to `init` before the walk is out of reach, and so is one forked after the freeze below ends.

use std::collections::HashSet;
use std::process::{Command, Stdio};

/// Most freeze rounds before the kill goes ahead with what was found: a fork bomb cannot keep it waiting.
const ROUNDS: usize = 8;

struct Row {
    pid: i32,
    ppid: i32,
    pgid: i32,
}

fn table() -> Option<Vec<Row>> {
    let mut command = Command::new("ps");
    command
        .args(["-A", "-o", "pid=,ppid=,pgid="])
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    // A short-lived helper, but every spawn goes through the one gate: it never sees account secrets.
    crate::host::child_env::for_tool(&mut command);
    let output = command.output().ok().filter(|o| o.status.success())?;
    let text = String::from_utf8_lossy(&output.stdout);
    Some(
        text.lines()
            .filter_map(|line| {
                let mut numbers = line.split_whitespace().map(|n| n.parse::<i32>().ok());
                Some(Row {
                    pid: numbers.next()??,
                    ppid: numbers.next()??,
                    pgid: numbers.next()??,
                })
            })
            .collect(),
    )
}

/// Whether `pid` exists (a zombie counts), by a probe that sends no signal.
fn exists(pid: i32) -> bool {
    // SAFETY: signal 0 only probes for existence and touches no memory.
    let probed = unsafe { libc::kill(pid, 0) };
    probed == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Every process below `leader` (not `leader` itself). Empty when `leader` is gone, or is not the leader of its own group, which
/// means its pid was reused by something this owner never started.
pub fn descendants(leader: i32) -> Vec<i32> {
    if !exists(leader) {
        return Vec::new();
    }
    let Some(rows) = table() else {
        return Vec::new();
    };
    if !rows.iter().any(|r| r.pid == leader && r.pgid == leader) {
        return Vec::new();
    }
    let mut found: HashSet<i32> = HashSet::from([leader]);
    let mut order = Vec::new();
    loop {
        let before = found.len();
        for row in &rows {
            if found.contains(&row.ppid) && found.insert(row.pid) {
                order.push(row.pid);
            }
        }
        if found.len() == before {
            return order;
        }
    }
}

fn signal_all(pids: &[i32], signal: i32) {
    for &pid in pids {
        if pid > 1 {
            // SAFETY: `kill` takes plain integers; ESRCH after an exit is harmless.
            unsafe {
                libc::kill(pid, signal);
            }
        }
    }
}

/// SIGTERM to every descendant (the group is signalled by the caller).
pub fn terminate(leader: i32) {
    signal_all(&descendants(leader), libc::SIGTERM);
}

/// Stops the group and every descendant, again and again until a round finds no new one, so nothing forks while the tree is being
/// found, then kills them all.
pub fn kill(leader: i32) {
    let mut known: Vec<i32> = Vec::new();
    for round in 0..ROUNDS {
        if round == 0 {
            // SAFETY: a negative pid addresses the group the caller owns.
            unsafe {
                libc::kill(-leader, libc::SIGSTOP);
            }
        }
        let fresh: Vec<i32> = descendants(leader)
            .into_iter()
            .filter(|pid| !known.contains(pid))
            .collect();
        if fresh.is_empty() {
            break;
        }
        signal_all(&fresh, libc::SIGSTOP);
        known.extend(fresh);
    }
    // SAFETY: as above.
    unsafe {
        libc::kill(-leader, libc::SIGKILL);
    }
    signal_all(&known, libc::SIGKILL);
}
