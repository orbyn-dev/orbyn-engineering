//! Unix: the descendants of a process group leader. A process group does not hold a child that calls `setsid` or moves itself to
//! another group, so the owner of a tree also reads the process table (no helper process: `/proc` on Linux, `libproc` on macOS) and
//! signals the descendants it finds.
//!
//! What this covers, and what it does not:
//! - The walk runs only while the leader is **unreaped** (its pid cannot have been reused), so every ancestry it follows is real.
//! - Every pid found is **recorded with its start time**, and a later kill signals a recorded pid only if a fresh table still shows
//!   the same pid with the same start time, never pid 1, this process, or this process's own group. A descendant seen by
//!   `terminate` is therefore still finished by the `kill` on drop, after its parent exited and it was reparented to `init`.
//! - Not covered: a process orphaned to `init` before any walk saw it (a double fork whose middle process has already exited), and
//!   one forked after the last freeze round. Closing those needs a container such as a cgroup or a launchd job.

/// Most freeze rounds before the kill goes ahead with what was found: a fork bomb cannot keep it waiting.
const ROUNDS: usize = 8;

/// A process by pid and start time: a reused pid has another start time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Member {
    pid: i32,
    start: u64,
}

struct Row {
    pid: i32,
    ppid: i32,
    pgid: i32,
    start: u64,
}

#[cfg(target_os = "linux")]
fn table() -> Vec<Row> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let pid: i32 = entry.file_name().to_str()?.parse().ok()?;
            let text = std::fs::read_to_string(entry.path().join("stat")).ok()?;
            // `pid (comm) state ppid pgrp ...`: the name may hold spaces and parentheses, so read after the last `)`.
            let rest = &text[text.rfind(')')? + 1..];
            let fields: Vec<&str> = rest.split_whitespace().collect();
            Some(Row {
                pid,
                ppid: fields.get(1)?.parse().ok()?,
                pgid: fields.get(2)?.parse().ok()?,
                start: fields.get(19)?.parse().ok()?,
            })
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn table() -> Vec<Row> {
    use std::mem::size_of;
    // SAFETY: the buffers are sized from the calls' own answers and outlive them; `proc_bsdinfo` is plain data.
    unsafe {
        let count = libc::proc_listallpids(std::ptr::null_mut(), 0);
        if count <= 0 {
            return Vec::new();
        }
        let mut pids: Vec<libc::pid_t> = vec![0; count as usize + 64];
        let filled = libc::proc_listallpids(
            pids.as_mut_ptr().cast(),
            (pids.len() * size_of::<libc::pid_t>()) as libc::c_int,
        );
        pids.truncate(filled.max(0) as usize);
        pids.into_iter()
            .filter(|pid| *pid > 0)
            .filter_map(|pid| {
                let mut info: libc::proc_bsdinfo = std::mem::zeroed();
                let size = size_of::<libc::proc_bsdinfo>() as libc::c_int;
                let got = libc::proc_pidinfo(
                    pid,
                    libc::PROC_PIDTBSDINFO,
                    0,
                    (&raw mut info).cast(),
                    size,
                );
                (got == size).then_some(Row {
                    pid,
                    ppid: info.pbi_ppid as i32,
                    pgid: info.pbi_pgid as i32,
                    start: info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec,
                })
            })
            .collect()
    }
}

/// Other Unix systems: no table, so only the process group is signalled.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn table() -> Vec<Row> {
    Vec::new()
}

/// A pid that may be signalled by this module: never `init` or below, this process, or this process's own group.
fn signalable(pid: i32) -> bool {
    // SAFETY: both calls read the caller's own ids.
    pid > 1 && pid != unsafe { libc::getpid() } && pid != unsafe { libc::getpgrp() }
}

fn send(pid: i32, signal: i32) {
    if signalable(pid) {
        // SAFETY: `kill` takes plain integers; ESRCH after an exit is harmless.
        unsafe {
            libc::kill(pid, signal);
        }
    }
}

/// Every process below `leader` (not `leader`), found by parent links. The caller guarantees `leader` is unreaped, so the pid is its
/// own; the row must also lead its own group, as the owner made it.
fn below(rows: &[Row], leader: i32) -> Vec<Member> {
    if !rows.iter().any(|r| r.pid == leader && r.pgid == leader) {
        return Vec::new();
    }
    let mut found = vec![leader];
    let mut members = Vec::new();
    let mut at = 0;
    while at < found.len() {
        let parent = found[at];
        at += 1;
        for row in rows.iter().filter(|r| r.ppid == parent) {
            if !found.contains(&row.pid) {
                found.push(row.pid);
                members.push(Member {
                    pid: row.pid,
                    start: row.start,
                });
            }
        }
    }
    members
}

fn remember(seen: &mut Vec<Member>, found: Vec<Member>) -> Vec<Member> {
    let fresh: Vec<Member> = found.into_iter().filter(|m| !seen.contains(m)).collect();
    seen.extend(fresh.iter().copied());
    fresh
}

/// Records the descendants of an unreaped `leader` without signalling anything, so a later kill still finds one that is reparented.
pub fn track(leader: i32, seen: &mut Vec<Member>) {
    remember(seen, below(&table(), leader));
}

/// Records the descendants of an unreaped `leader` and sends each SIGTERM; the caller signals the group. They are not stopped:
/// a stopped process would hold the SIGTERM undelivered, and an orderly shutdown needs it delivered.
pub fn terminate(leader: i32, seen: &mut Vec<Member>) {
    let fresh = remember(seen, below(&table(), leader));
    for member in fresh {
        send(member.pid, libc::SIGTERM);
    }
}

/// Kills the group and every descendant. While `leader_unreaped`, the group and the descendants are stopped round after round until
/// one finds nothing new, so nothing forks while the tree is found. Then every recorded member that the process table still shows
/// with its recorded start time is killed, and last the group.
pub fn kill(leader: i32, leader_unreaped: bool, seen: &mut Vec<Member>) {
    if leader_unreaped {
        // SAFETY: a negative pid addresses the group the caller owns.
        unsafe {
            libc::kill(-leader, libc::SIGSTOP);
        }
        for _ in 0..ROUNDS {
            let fresh = remember(seen, below(&table(), leader));
            if fresh.is_empty() {
                break;
            }
            for member in fresh {
                send(member.pid, libc::SIGSTOP);
            }
        }
    }
    if !seen.is_empty() {
        let rows = table();
        for member in seen.iter() {
            if rows
                .iter()
                .any(|r| r.pid == member.pid && r.start == member.start)
            {
                send(member.pid, libc::SIGKILL);
            }
        }
    }
    // SAFETY: as above.
    unsafe {
        libc::kill(-leader, libc::SIGKILL);
    }
}
