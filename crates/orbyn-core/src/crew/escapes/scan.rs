//! The possible-escape scan (slice 4k, PR 3; docs/crew/escapes.md, "Possible-escape detection"). It looks at merges that have no
//! escape row yet and writes **possible** escapes only; nothing here confirms, and nothing feeds the breaker, the Router or a gate.
//!
//! Three signals, one per merge (the first that fires; a merge has at most one escape):
//!
//! 1. **Revert made outside the crew.** A later commit on the target branch whose message names the merge commit and is a revert
//!    (`Revert "..."` subject or a `This reverts commit <merge>` body). Not offered when the revert was itself reverted or reapplied
//!    (a re-land) or says it is a bisect. The crew's own revert never gets here: it already has its escape row (the schema's trigger).
//! 2. **Follow-up fix on the same lines.** A later commit within `follow_up_days` whose subject starts `fix` and that changes a line
//!    the merge added (git blame at the fix's parent names a commit the merge brought in). Highest false-positive risk of all; the
//!    row's reason says so in plain facts and a person decides.
//! 3. **Post-merge check failure.** `watch = broken`, not reverted: the device's own repository checks only (no git needed).
//!
//! When more than one fires for a merge the order is 1, 3, 2: the strongest evidence that needs the least git comes first.
//!
//! **Read-only and local.** Only `git` on the project's own checkout, hardened (no repository filter, diff converter or attribute
//! runs a program): `merge-base`, `log`, `diff`, `diff-tree`, `rev-list`, `blame`, with literal pathspecs and the branch read as `refs/heads/<branch>`. No worker, no engine call, no network, no write to a repository.
//! A GET never runs it; `run_crew_escape_scan` does, once when the service starts and on every Crew watch interval.
//!
//! **Bounds.** At most [`MERGES_PER_SCAN`] merges, [`COMMITS_PER_MERGE`] later commits per merge, [`GIT_CALLS_PER_SCAN`] git calls and
//! [`SCAN_SECONDS`] seconds per scan, each git call cut at the scan's deadline and capped in output bytes; what is not reached stays a
//! candidate. A merge with no signal never gets a row, so the scan keeps a cursor (`CURSOR_KEY` in `settings`) and the next scan resumes
//! after the last merge looked at, wrapping to the newest at the end: every merge is reached over successive scans. A project whose
//! log is at its 5,000 escapes or 20,000 events is refused whole, before any git runs and with nothing written.
//!
//! **Idempotent.** A merge with an escape row of any state is not a candidate, a dismissed escape stays dismissed, and the writer
//! adds one row per merge, so a repeat scan adds nothing.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use orbyn_contracts::EscapeSource;

use super::thresholds::escape_thresholds;
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::source_control::git::git_hardened_capped;
use crate::store::{ESCAPES_PER_PROJECT_MAX, EVENTS_PER_PROJECT_MAX, MergeRecord, PossibleEscape};

/// The most merges one scan looks at.
pub const MERGES_PER_SCAN: u32 = 50;
/// The most later commits read for one merge.
pub const COMMITS_PER_MERGE: u32 = 200;
/// The most git calls one scan makes (each is a few short processes).
pub const GIT_CALLS_PER_SCAN: u32 = 300;
/// The longest one scan runs.
pub const SCAN_SECONDS: u64 = 60;
const FIXES_PER_MERGE: usize = 5;
const FILES_PER_FIX: usize = 10;
const HUNKS_PER_FILE: usize = 20;
const MERGE_FILES_MAX: usize = 2000;
const INTRODUCED_MAX: usize = 500;
/// Output caps per git call, in bytes.
const LOG_BYTES: usize = 1 << 20;
const LIST_BYTES: usize = 1 << 20;
const DIFF_BYTES: usize = 256 << 10;
const BLAME_BYTES: usize = 1 << 20;
/// Where the next scan resumes (merge time and id of the last merge looked at), in `settings`; never an account preference.
pub const CURSOR_KEY: &str = "crew_escape_scan_cursor";

/// The bounds of one scan. [`Limits::default`] is what the service uses; tests pass smaller ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub merges: u32,
    pub calls: u32,
    /// The scan's wall-clock bound, in seconds.
    pub seconds: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            merges: MERGES_PER_SCAN,
            calls: GIT_CALLS_PER_SCAN,
            seconds: SCAN_SECONDS,
        }
    }
}

/// What one scan did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScanOutcome {
    /// Merges looked at.
    pub scanned: u32,
    /// New possible escapes written.
    pub found: u32,
    /// Merges that already had an escape by the time of the write (left as they were).
    pub already_held: u32,
    /// Merges git could not be read for (a missing repository, a rewritten branch) and nothing else flagged.
    pub skipped: u32,
    /// Merges of projects whose log is full, whose write was refused.
    pub refused: u32,
    /// A bound was reached before every candidate was looked at.
    pub truncated: bool,
}

/// A commit after the merge, on the target branch.
struct Later {
    id: String,
    at: i64,
    message: String,
}

impl Later {
    fn subject(&self) -> &str {
        self.message.lines().next().unwrap_or("").trim()
    }
}

#[derive(Debug)]
enum Stop {
    /// The scan's git call count is spent.
    Calls,
    /// The scan's time is spent, or this call ran past it.
    Deadline,
    /// Git could not answer for this merge.
    Git,
}

struct Budget {
    calls: u32,
    deadline: Instant,
}

impl Budget {
    fn take(&mut self) -> std::result::Result<(), Stop> {
        if self.calls == 0 {
            return Err(Stop::Calls);
        }
        if Instant::now() >= self.deadline {
            return Err(Stop::Deadline);
        }
        self.calls -= 1;
        Ok(())
    }

    /// One hardened git call, bounded in time by what is left of the scan and in size by `cap`. Every path argument is literal.
    async fn out(
        &mut self,
        repo: &Path,
        args: &[&str],
        cap: usize,
    ) -> std::result::Result<String, Stop> {
        self.take()?;
        let left = self.deadline.saturating_duration_since(Instant::now());
        let mut full: Vec<&str> = vec!["--literal-pathspecs"];
        full.extend_from_slice(args);
        match tokio::time::timeout(left, git_hardened_capped(repo, &full, cap)).await {
            Err(_) => Err(Stop::Deadline),
            Ok(Err(_)) => Err(Stop::Git),
            Ok(Ok(bytes)) => Ok(String::from_utf8_lossy(&bytes).into_owned()),
        }
    }
}

fn is_id(text: &str) -> bool {
    matches!(text.len(), 40 | 64) && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn is_revert(commit: &Later) -> bool {
    commit.subject().starts_with("Revert ") || commit.message.contains("This reverts commit ")
}

fn short(id: &str) -> &str {
    &id[..id.len().min(12)]
}

/// The leading word of a subject is `fix`, `fixes` or `fixed` (`fix:`, `fix(core):`, `Fixes the lint`), never `fixture`.
fn is_fix_subject(subject: &str) -> bool {
    let word: String = subject
        .chars()
        .take_while(char::is_ascii_alphabetic)
        .collect::<String>()
        .to_ascii_lowercase();
    matches!(word.as_str(), "fix" | "fixes" | "fixed")
}

/// Commits after the merge on the branch, newest first. The log is NUL-separated (`\0 id \0 time \0 message`): a commit message
/// cannot hold a NUL, so no message can forge or split an entry.
fn parse_log(out: &str) -> Vec<Later> {
    let mut parts = out.split('\0');
    if parts.next() != Some("") {
        return Vec::new();
    }
    let mut commits = Vec::new();
    while let (Some(id), Some(at), Some(message)) = (parts.next(), parts.next(), parts.next()) {
        let (Ok(at), true) = (at.parse::<i64>(), is_id(id)) else {
            return Vec::new();
        };
        commits.push(Later {
            id: id.to_owned(),
            at,
            message: message.trim().to_owned(),
        });
    }
    commits
}

/// Signal 1. The oldest later commit that reverts the merge, unless it is a re-land or a bisect.
fn external_revert<'a>(merge: &str, later: &'a [Later]) -> Option<&'a Later> {
    later.iter().rev().find(|c| {
        if !c.message.contains(merge) || !is_revert(c) {
            return false;
        }
        let lower = c.message.to_ascii_lowercase();
        if lower.contains("bisect") {
            return false;
        }
        let relanded = later.iter().any(|d| {
            d.id != c.id && d.message.contains(&c.id) && {
                let words = d.message.to_ascii_lowercase();
                is_revert(d)
                    || words.contains("reapply")
                    || words.contains("re-land")
                    || words.contains("reland")
            }
        });
        !relanded
    })
}

/// The old-side line ranges of a `-U0` diff. A pure insertion (count 0) has no old line and is left out: a fix that only adds
/// lines next to the merge's lines changes none of them.
fn old_ranges(diff: &str) -> Vec<(u32, u32)> {
    diff.lines()
        .filter_map(|line| {
            let old = line.strip_prefix("@@ -")?.split(' ').next()?;
            let (start, count) = old.split_once(',').unwrap_or((old, "1"));
            let (start, count): (u32, u32) = (start.parse().ok()?, count.parse().ok()?);
            (count > 0 && start > 0).then(|| (start, start + count - 1))
        })
        .take(HUNKS_PER_FILE)
        .collect()
}

/// The commits `git blame --porcelain` names, from its block headers.
fn blamed(porcelain: &str) -> BTreeSet<String> {
    porcelain
        .lines()
        .filter_map(|line| {
            let mut words = line.split(' ');
            let id = words.next()?;
            let line_no = words.next()?;
            (is_id(id) && line_no.bytes().all(|b| b.is_ascii_digit()) && !line_no.is_empty())
                .then(|| id.to_owned())
        })
        .collect()
}

/// The branch as a full ref, so a tag or another ref with the same short name can never stand in for it.
fn branch_ref(branch: &str) -> Option<String> {
    let ok = !branch.is_empty()
        && !branch.starts_with('-')
        && !branch.chars().any(|c| c.is_whitespace() || c.is_control());
    ok.then(|| {
        if branch.starts_with("refs/") {
            branch.to_owned()
        } else {
            format!("refs/heads/{branch}")
        }
    })
}

/// Reads the later commits of one merge. An error means git could not say (not an ancestor of the branch, or unreadable).
async fn later_commits(
    budget: &mut Budget,
    repo: &Path,
    record: &MergeRecord,
) -> std::result::Result<Vec<Later>, Stop> {
    let Some(branch) = branch_ref(&record.branch) else {
        return Err(Stop::Git);
    };
    if !is_id(&record.merge_commit) {
        return Err(Stop::Git);
    }
    // Exit 0 only when the merge is on the branch; any other exit, or a missing object, is an error here.
    budget
        .out(
            repo,
            &["merge-base", "--is-ancestor", &record.merge_commit, &branch],
            LIST_BYTES,
        )
        .await?;
    let range = format!("{}..{branch}", record.merge_commit);
    let count = format!("--max-count={COMMITS_PER_MERGE}");
    let out = budget
        .out(
            repo,
            &[
                "log",
                "--no-merges",
                &count,
                "--format=%x00%H%x00%ct%x00%B",
                &range,
                "--",
            ],
            LOG_BYTES,
        )
        .await?;
    // A commit object can carry a NUL in its message, so the parsed ids are only trusted when git itself lists them as later commits.
    let listed = budget
        .out(
            repo,
            &["rev-list", "--no-merges", &count, &range, "--"],
            LIST_BYTES,
        )
        .await?;
    let real: BTreeSet<&str> = listed.lines().map(str::trim).collect();
    Ok(parse_log(&out)
        .into_iter()
        .filter(|c| real.contains(c.id.as_str()))
        .collect())
}

/// Signal 2: a later `fix` commit changed a line the merge added. `Ok(None)` when none did.
async fn follow_up_fix<'a>(
    budget: &mut Budget,
    repo: &Path,
    record: &MergeRecord,
    later: &'a [Later],
    follow_up_days: u32,
) -> std::result::Result<Option<&'a Later>, Stop> {
    let until = record
        .merged_unix
        .saturating_add(i64::from(follow_up_days) * 86_400);
    let fixes: Vec<&Later> = later
        .iter()
        .rev()
        .filter(|c| {
            c.at > record.merged_unix
                && c.at <= until
                && is_fix_subject(c.subject())
                && !is_revert(c)
        })
        .take(FIXES_PER_MERGE)
        .collect();
    if fixes.is_empty() {
        return Ok(None);
    }
    let (before, merge) = (record.before_commit.as_str(), record.merge_commit.as_str());
    if !is_id(before) {
        return Err(Stop::Git);
    }
    let files_out = budget
        .out(
            repo,
            &[
                "diff",
                "--name-only",
                "-z",
                "--no-ext-diff",
                "--no-textconv",
                before,
                merge,
                "--",
            ],
            LIST_BYTES,
        )
        .await?;
    let merged_files: BTreeSet<&str> = files_out
        .split('\0')
        .filter(|f| !f.is_empty())
        .take(MERGE_FILES_MAX)
        .collect();
    let mut shared: Vec<(&Later, Vec<String>)> = Vec::new();
    for fix in fixes {
        let names = match budget
            .out(
                repo,
                &[
                    "diff-tree",
                    "--no-commit-id",
                    "--name-only",
                    "-r",
                    "-z",
                    "--no-ext-diff",
                    &fix.id,
                ],
                LIST_BYTES,
            )
            .await
        {
            Ok(names) => names,
            Err(Stop::Git) => continue,
            Err(stop) => return Err(stop),
        };
        let both: Vec<String> = names
            .split('\0')
            .filter(|f| merged_files.contains(f))
            .take(FILES_PER_FIX)
            .map(str::to_owned)
            .collect();
        if !both.is_empty() {
            shared.push((fix, both));
        }
    }
    if shared.is_empty() {
        return Ok(None);
    }
    let range = format!("{before}..{merge}");
    let max = format!("--max-count={}", INTRODUCED_MAX + 1);
    let introduced: BTreeSet<String> = budget
        .out(repo, &["rev-list", &max, &range], LIST_BYTES)
        .await?
        .lines()
        .map(|l| l.trim().to_owned())
        .collect();
    if introduced.len() > INTRODUCED_MAX {
        return Err(Stop::Git);
    }
    for (fix, files) in shared {
        let parent = format!("{}^", fix.id);
        for file in files {
            let diff = match budget
                .out(
                    repo,
                    &[
                        "diff",
                        "-U0",
                        "--no-color",
                        "--no-ext-diff",
                        "--no-textconv",
                        &parent,
                        &fix.id,
                        "--",
                        &file,
                    ],
                    DIFF_BYTES,
                )
                .await
            {
                Ok(diff) => diff,
                Err(Stop::Git) => continue,
                Err(stop) => return Err(stop),
            };
            let ranges = old_ranges(&diff);
            if ranges.is_empty() {
                continue;
            }
            let mut args: Vec<String> =
                vec!["blame".into(), "--porcelain".into(), "--no-textconv".into()];
            for (from, to) in ranges {
                args.push("-L".into());
                args.push(format!("{from},{to}"));
            }
            args.extend([parent.clone(), "--".into(), file]);
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            match budget.out(repo, &refs, BLAME_BYTES).await {
                Ok(porcelain) => {
                    if blamed(&porcelain).iter().any(|id| introduced.contains(id)) {
                        return Ok(Some(fix));
                    }
                }
                Err(Stop::Git) => {}
                Err(stop) => return Err(stop),
            }
        }
    }
    Ok(None)
}

/// What one candidate showed: a possible escape, nothing, or that git could not be read for it.
enum Look {
    Found(PossibleEscape),
    Nothing,
    Unread,
}

async fn look(
    budget: &mut Budget,
    record: &MergeRecord,
    project: &str,
    follow_up_days: u32,
) -> std::result::Result<Look, Stop> {
    let make = |source, cause: String, reason: String| {
        Look::Found(PossibleEscape {
            project_id: project.to_owned(),
            merge_task_id: record.task_id.clone(),
            source,
            cause,
            reason,
        })
    };
    let repo = Path::new(&record.repo);
    let later = if repo.is_dir() {
        match later_commits(budget, repo, record).await {
            Ok(later) => Some(later),
            Err(Stop::Git) => None,
            Err(stop) => return Err(stop),
        }
    } else {
        None
    };
    if let Some(revert) = later
        .as_deref()
        .and_then(|l| external_revert(&record.merge_commit, l))
    {
        return Ok(make(
            EscapeSource::Revert,
            format!("revert:{}", revert.id),
            format!(
                "Commit {} on the branch reverts this merge.",
                short(&revert.id)
            ),
        ));
    }
    if record.watch == "broken"
        && record.reverted_commit.is_none()
        && record.review_state != "reverted"
    {
        return Ok(make(
            EscapeSource::PostMergeCheck,
            format!("post_merge_check:{}", record.merge_commit),
            "A repository check failed on the branch after this merge, and the merge was not reverted.".into(),
        ));
    }
    let Some(later) = later else {
        return Ok(Look::Unread);
    };
    match follow_up_fix(budget, repo, record, &later, follow_up_days).await {
        Ok(Some(fix)) => Ok(make(
            EscapeSource::FollowUpFix,
            format!("follow_up_fix:{}", fix.id),
            format!(
                "Commit {}, a fix within {follow_up_days} days, changed lines this merge added. It may be ordinary follow-up work.",
                short(&fix.id)
            ),
        )),
        Ok(None) => Ok(Look::Nothing),
        Err(Stop::Git) => Ok(Look::Unread),
        Err(stop) => Err(stop),
    }
}

impl Core {
    /// One scan at `now`: looks at the newest merges with no escape yet and writes the possible escapes it finds, one transaction
    /// per project. A person's confirm is never involved. See the module notes for the three signals and the bounds.
    pub async fn crew_escape_scan(&self, now: i64) -> Result<ScanOutcome> {
        self.crew_escape_scan_within(now, Limits::default()).await
    }

    /// [`Self::crew_escape_scan`] with explicit bounds.
    pub async fn crew_escape_scan_within(&self, now: i64, limits: Limits) -> Result<ScanOutcome> {
        self.crew_preview_only()?;
        let thresholds = escape_thresholds(&self.store)?;
        let since = now - i64::from(thresholds.report_days) * 86_400;
        // Resume after the last merge the previous scan looked at, so merges with no signal (which never get a row) are not looked
        // at again and again while older ones starve. When the rest of the list is shorter than a scan, wrap to the newest.
        let (cursor, damaged) = self.scan_cursor()?;
        let want = limits.merges + 1;
        let mut candidates = self
            .store
            .crew_escape_scan_candidates(since, want, cursor)?;
        if cursor.is_some() && candidates.len() < want as usize {
            let top = self.store.crew_escape_scan_candidates(since, want, None)?;
            for item in top {
                if candidates.len() >= want as usize {
                    break;
                }
                if !candidates.iter().any(|(m, _)| m.task_id == item.0.task_id) {
                    candidates.push(item);
                }
            }
        }
        let mut outcome = ScanOutcome {
            truncated: candidates.len() > limits.merges as usize,
            ..ScanOutcome::default()
        };
        let mut full: BTreeMap<String, bool> = BTreeMap::new();
        let mut budget = Budget {
            calls: limits.calls,
            deadline: Instant::now() + Duration::from_secs(limits.seconds),
        };
        let mut found: BTreeMap<String, Vec<PossibleEscape>> = BTreeMap::new();
        // Every merge passed over, in order, with its project: the cursor only ever moves past these.
        let mut done: Vec<(&MergeRecord, &String)> = Vec::new();
        for (record, project) in candidates.iter().take(limits.merges as usize) {
            let is_full = match full.get(project) {
                Some(known) => *known,
                None => {
                    let (escapes, events) = self.store.crew_escape_room(project)?;
                    let is_full =
                        escapes >= ESCAPES_PER_PROJECT_MAX || events >= EVENTS_PER_PROJECT_MAX;
                    if is_full {
                        tracing::warn!(project = %project, "the escape log is full; the scan wrote nothing for it");
                    }
                    full.insert(project.clone(), is_full);
                    is_full
                }
            };
            if is_full {
                outcome.refused += 1;
                done.push((record, project));
                continue;
            }
            match look(&mut budget, record, project, thresholds.follow_up_days).await {
                Ok(Look::Found(item)) => found.entry(project.clone()).or_default().push(item),
                Ok(Look::Nothing) => {}
                Ok(Look::Unread) => outcome.skipped += 1,
                // The very first merge of a scan that cannot finish in the time there is would be the first merge of the next scan
                // too: count it as unread and move on, so one merge can never pin the scan.
                Err(Stop::Deadline) if done.is_empty() => outcome.skipped += 1,
                Err(_) => {
                    outcome.truncated = true;
                    break;
                }
            }
            outcome.scanned += 1;
            done.push((record, project));
        }
        // Rows first, then the cursor: a crash between them only rescans merges that already have a row.
        for (project, items) in found {
            match self.store.record_possible_escapes(&project, &items, now) {
                Ok(added) => {
                    outcome.found += added.added;
                    outcome.already_held += added.already_held;
                }
                Err(CoreError::Invalid(why)) => {
                    tracing::warn!(project = %project, %why, "the escape scan wrote nothing for a project");
                    outcome.refused += u32::try_from(items.len()).unwrap_or(u32::MAX);
                }
                Err(other) => return Err(other),
            }
        }
        // A batch the writer refused is treated like a project that was full before git ran: counted as refused and passed over; the
        // next wrap tries again. Holding the cursor there would repeat the same window for ever.
        let finished = !outcome.truncated && done.len() == candidates.len();
        let next = if finished {
            Some(String::new())
        } else {
            done.last()
                .map(|(record, _)| format!("{}:{}", record.merged_unix, record.id))
                .or_else(|| damaged.then(String::new))
        };
        if let Some(next) = next
            && self.store.setting(CURSOR_KEY)?.unwrap_or_default() != next
        {
            self.store.set_setting(CURSOR_KEY, &next)?;
        }
        Ok(outcome)
    }

    /// Where the last truncated scan stopped, and whether the stored value was damaged. Absent or empty means start from the newest;
    /// a damaged value reads as absent and is overwritten by this scan.
    fn scan_cursor(&self) -> Result<(Option<(i64, i64)>, bool)> {
        let Some(text) = self.store.setting(CURSOR_KEY)?.filter(|t| !t.is_empty()) else {
            return Ok((None, false));
        };
        let parsed = text
            .split_once(':')
            .and_then(|(at, id)| Some((at.parse().ok()?, id.parse().ok()?)));
        Ok((parsed, parsed.is_none()))
    }

    /// The service's scan: once at start, then on every Crew watch interval (the watch's tick; none when the interval is off).
    /// Nothing else calls the scan, and no route does. Stops with `stop`. A failed scan is logged and the next tick tries again.
    pub async fn run_crew_escape_scan(
        self: &Arc<Self>,
        stop: tokio_util::sync::CancellationToken,
    ) -> Result<()> {
        if !self.crew_enabled() {
            return Ok(());
        }
        let every = self.config.crew_merge_watch_interval;
        let mut timer = every.map(|every| {
            let mut timer = tokio::time::interval_at(tokio::time::Instant::now() + every, every);
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            timer
        });
        loop {
            tokio::select! {
                biased;
                _ = stop.cancelled() => return Ok(()),
                result = self.crew_escape_scan(crate::util::unix_now()) => {
                    if let Err(error) = result {
                        tracing::warn!(%error, "the escape scan needs another try");
                    }
                }
            }
            let Some(timer) = timer.as_mut() else {
                return Ok(());
            };
            tokio::select! {
                biased;
                _ = stop.cancelled() => return Ok(()),
                _ = timer.tick() => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    #[test]
    fn the_log_is_nul_separated_so_a_message_cannot_forge_an_entry() {
        let id = "a".repeat(40);
        let other = "b".repeat(40);
        let out = format!(
            "\0{id}\0100\0docs: x\x1e{other}\x1f1\x1ffix: forged\n\0{other}\0200\0second\n"
        );
        let commits = parse_log(&out);
        assert_eq!(commits.len(), 2);
        assert_eq!((commits[0].id.as_str(), commits[0].at), (id.as_str(), 100));
        assert!(commits[0].message.contains("forged"));
        assert_eq!(
            (commits[1].id.as_str(), commits[1].message.as_str()),
            (other.as_str(), "second")
        );
        // Anything that does not parse is no log at all, never a guess.
        assert!(parse_log("garbage").is_empty());
        assert!(parse_log(&format!("\0{id}\0notatime\0m")).is_empty());
    }

    #[test]
    fn a_pure_insertion_has_no_old_lines() {
        let diff = "@@ -3,0 +4 @@ x\n@@ -7 +8 @@\n@@ -10,2 +11,2 @@\n";
        assert_eq!(old_ranges(diff), vec![(7, 7), (10, 11)]);
    }

    fn repo() -> (TempDir, std::path::PathBuf) {
        let dir = TempDir::new();
        let path = dir.path().join("r");
        std::fs::create_dir_all(&path).unwrap();
        for args in [
            vec!["-c", "init.defaultBranch=main", "init", "-q"],
            vec![
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.test",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "base",
            ],
        ] {
            crate::testing::run_git(&path, &args);
        }
        (dir, path)
    }

    #[tokio::test]
    async fn a_call_is_cut_at_the_scans_deadline_and_at_its_output_cap() {
        let (_dir, path) = repo();
        let mut spent = Budget {
            calls: 10,
            deadline: Instant::now(),
        };
        assert!(matches!(
            spent.out(&path, &["log"], 1 << 20).await,
            Err(Stop::Deadline)
        ));
        let mut roomy = Budget {
            calls: 10,
            deadline: Instant::now() + Duration::from_secs(30),
        };
        assert!(
            roomy
                .out(&path, &["log"], 1 << 20)
                .await
                .unwrap()
                .contains("base")
        );
        // The output is past its cap: the call is refused, not cut to a partial answer.
        assert!(matches!(
            roomy.out(&path, &["log"], 10).await,
            Err(Stop::Git)
        ));
        // A call count of zero is the same bound.
        let mut none = Budget {
            calls: 0,
            deadline: Instant::now() + Duration::from_secs(30),
        };
        assert!(matches!(
            none.out(&path, &["log"], 1 << 20).await,
            Err(Stop::Calls)
        ));
    }

    /// A call that fills the stderr pipe many times over still returns: stdout and stderr are read together.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_chatty_stderr_does_not_stall_the_call() {
        let (_dir, path) = repo();
        let mut budget = Budget {
            calls: 10,
            deadline: Instant::now() + Duration::from_secs(20),
        };
        let started = Instant::now();
        let out = budget
            .out(
                &path,
                &[
                    "-c",
                    "alias.flood=!head -c 400000 /dev/zero >&2; echo ok",
                    "flood",
                ],
                1 << 20,
            )
            .await
            .unwrap();
        assert_eq!(out.trim(), "ok");
        assert!(started.elapsed() < Duration::from_secs(15));
    }

    #[test]
    fn a_branch_is_a_full_ref() {
        assert_eq!(branch_ref("main").as_deref(), Some("refs/heads/main"));
        assert_eq!(branch_ref("refs/heads/x").as_deref(), Some("refs/heads/x"));
        assert!(
            branch_ref("-x").is_none() && branch_ref("a b").is_none() && branch_ref("").is_none()
        );
    }
}
