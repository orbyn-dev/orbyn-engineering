//! Unix: the descendants of a process group leader. A process group does not hold a child that calls `setsid` or moves itself to
//! another group, so the owner of a tree also reads the process table (no helper process: `/proc` on Linux, `libproc` on macOS) and
//! signals the descendants it finds.
//!
//! What this covers, and what it does not:
//! - A machine with no readable table (`/proc` missing or refused, `libproc` refusing, another Unix) falls back to the group alone.
//! - The walk runs only while the leader is **unreaped** (its pid cannot have been reused), so every ancestry it follows is real.
//! - Every pid found is **recorded with its start time**, and a later kill signals a recorded pid only if a fresh table still shows
//!   the same pid with the same start time, never pid 1, this process, or this process's own group. A descendant seen by
//!   `terminate` is therefore still finished by the `kill` on drop, after its parent exited and it was reparented to `init`.
//! - Linux also makes the leader a **subreaper** (`super::isolate`), so a process orphaned before any walk saw it — a double fork
//!   whose middle process has already exited — is adopted back under the leader rather than `init`, and the parent link the walk
//!   follows is a real one. macOS has no such flag: there an orphan of that shape leaves the tree, though the sandbox still confines
//!   it. A descendant forked after the last freeze round is out of reach on both. Closing either needs a container such as a cgroup or
//!   a launchd job.

use std::collections::HashSet;

/// Most freeze rounds before the kill goes ahead with what was found: a fork bomb cannot keep it waiting.
const ROUNDS: usize = 8;

/// A process by pid and start time: a reused pid has another start time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Member {
    pid: i32,
    start: u64,
}

#[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
struct Row {
    pid: i32,
    ppid: i32,
    start: u64,
}

/// One `/proc/<pid>/stat`: `pid (comm) state ppid pgrp ... starttime`. The name may hold spaces, parentheses and bytes that are not
/// UTF-8 (the kernel cuts it at 15 bytes, in the middle of a multibyte character), so it is read as bytes after the last `)`.
#[cfg(target_os = "linux")]
fn parse_stat(pid: i32, text: &[u8]) -> Option<Row> {
    let rest = &text[text.iter().rposition(|b| *b == b')')? + 1..];
    let rest = String::from_utf8_lossy(rest);
    let fields: Vec<&str> = rest.split_whitespace().collect();
    Some(Row {
        pid,
        ppid: fields.get(1)?.parse().ok()?,
        start: fields.get(19)?.parse().ok()?,
    })
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
            parse_stat(pid, &std::fs::read(entry.path().join("stat")).ok()?)
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
/// own whatever group it moved to.
fn below(rows: &[Row], leader: i32) -> Vec<Member> {
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

/// Drops what the table no longer shows (a dead pid with its start time never comes back), so the record stays as small as the live
/// tree. An empty table is a failed read, not an empty machine: nothing is dropped.
fn prune(seen: &mut HashSet<Member>, rows: &[Row]) {
    if rows.is_empty() {
        return;
    }
    let live: HashSet<Member> = rows
        .iter()
        .map(|r| Member {
            pid: r.pid,
            start: r.start,
        })
        .collect();
    seen.retain(|m| live.contains(m));
}

/// Records the descendants of an unreaped `leader` without signalling anything, so a later kill still finds one that is reparented.
pub fn track(leader: i32, seen: &mut HashSet<Member>) {
    let rows = table();
    prune(seen, &rows);
    seen.extend(below(&rows, leader));
}

/// Records the descendants of an unreaped `leader` and sends every one found SIGTERM; the caller signals the group. They are not
/// stopped: a stopped process would hold the SIGTERM undelivered, and an orderly shutdown needs it delivered.
pub fn terminate(leader: i32, seen: &mut HashSet<Member>) {
    let rows = table();
    prune(seen, &rows);
    for member in below(&rows, leader) {
        seen.insert(member);
        send(member.pid, libc::SIGTERM);
    }
}

/// Kills the group and every descendant. While `leader_unreaped`, the group and the descendants are stopped round after round until
/// one finds nothing new, so nothing forks while the tree is found. Everything stopped in this call is then killed without another
/// look (a stopped process cannot exit, so its pid is still its own, and a failed second read must not leave it stopped for good).
/// A member recorded earlier is killed only if a fresh table still shows it with its recorded start time. Last, the group.
pub fn kill(leader: i32, leader_unreaped: bool, seen: &mut HashSet<Member>) {
    let mut stopped: HashSet<Member> = HashSet::new();
    if leader_unreaped {
        // SAFETY: a negative pid addresses the group the caller owns.
        unsafe {
            libc::kill(-leader, libc::SIGSTOP);
        }
        for _ in 0..ROUNDS {
            let rows = table();
            let fresh: Vec<Member> = below(&rows, leader)
                .into_iter()
                .filter(|m| !stopped.contains(m))
                .collect();
            if fresh.is_empty() {
                break;
            }
            for member in fresh {
                send(member.pid, libc::SIGSTOP);
                stopped.insert(member);
            }
        }
    }
    for member in &stopped {
        send(member.pid, libc::SIGKILL);
    }
    let earlier: Vec<Member> = seen.difference(&stopped).copied().collect();
    if !earlier.is_empty() {
        let rows = table();
        let live: HashSet<Member> = rows
            .iter()
            .map(|r| Member {
                pid: r.pid,
                start: r.start,
            })
            .collect();
        for member in earlier.iter().filter(|m| live.contains(m)) {
            send(member.pid, libc::SIGKILL);
        }
    }
    seen.clear();
    // SAFETY: as above.
    unsafe {
        libc::kill(-leader, libc::SIGKILL);
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn a_stat_with_a_non_utf8_name_still_parses() {
        // 15 bytes of name cut inside a three-byte character, with a space and parentheses in it.
        let mut text = b"4242 (a b) \xe2\x82".to_vec();
        text.extend(b") S 7 4242 4242 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 987654 0 0");
        let row = parse_stat(4242, &text).expect("parsed");
        assert_eq!((row.ppid, row.start), (7, 987654));
    }
}
