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
//! runs a program): `merge-base`, `log`, `diff`, `rev-list`, `blame`. No worker, no engine call, no network, no write to a repository.
//! A GET never runs it; `run_crew_escape_scan` does, once when the service starts and on every Crew watch interval.
//!
//! **Bounds.** At most [`MERGES_PER_SCAN`] merges, [`COMMITS_PER_MERGE`] later commits per merge, [`GIT_CALLS_PER_SCAN`] git calls and
//! [`SCAN_SECONDS`] seconds per scan; what is not reached stays a candidate for the next scan (newest merges first). A project whose
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
use crate::source_control::git::{git_hardened, git_raw};
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

/// The bounds of one scan. [`Limits::default`] is what the service uses; tests pass smaller ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub merges: u32,
    pub calls: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            merges: MERGES_PER_SCAN,
            calls: GIT_CALLS_PER_SCAN,
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
    files: Vec<String>,
}

impl Later {
    fn subject(&self) -> &str {
        self.message.lines().next().unwrap_or("").trim()
    }
}

enum Stop {
    /// The call, merge or time bound is spent.
    Budget,
    /// Git could not answer for this merge.
    Git,
}

struct Budget {
    calls: u32,
    deadline: Instant,
}

impl Budget {
    fn take(&mut self) -> std::result::Result<(), Stop> {
        if self.calls == 0 || Instant::now() >= self.deadline {
            return Err(Stop::Budget);
        }
        self.calls -= 1;
        Ok(())
    }

    async fn out(&mut self, repo: &Path, args: &[&str]) -> std::result::Result<String, Stop> {
        self.take()?;
        let bytes = git_hardened(repo, args).await.map_err(|_| Stop::Git)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
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

/// Commits after the merge on the branch, newest first, with their files.
fn parse_log(out: &str) -> Vec<Later> {
    out.split('\u{1e}')
        .filter(|chunk| !chunk.trim().is_empty())
        .filter_map(|chunk| {
            let mut parts = chunk.splitn(4, '\u{1f}');
            let id = parts.next()?.trim().to_owned();
            let at = parts.next()?.trim().parse().ok()?;
            let message = parts.next()?.trim().to_owned();
            let files = parts
                .next()
                .unwrap_or("")
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_owned)
                .collect();
            is_id(&id).then_some(Later {
                id,
                at,
                message,
                files,
            })
        })
        .collect()
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

/// The old-side line ranges of a `-U0` diff. A pure insertion names the line it follows.
fn old_ranges(diff: &str) -> Vec<(u32, u32)> {
    diff.lines()
        .filter_map(|line| {
            let old = line.strip_prefix("@@ -")?.split(' ').next()?;
            let (start, count) = old.split_once(',').unwrap_or((old, "1"));
            let (start, count): (u32, u32) = (start.parse().ok()?, count.parse().ok()?);
            let first = start.max(1);
            Some(if count == 0 {
                (first, first)
            } else {
                (first, start + count - 1)
            })
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

fn valid_branch(branch: &str) -> bool {
    !branch.is_empty()
        && !branch.starts_with('-')
        && !branch.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Reads the later commits of one merge. `None` means git could not say (not an ancestor of the branch, or unreadable).
async fn later_commits(
    budget: &mut Budget,
    repo: &Path,
    record: &MergeRecord,
) -> std::result::Result<Vec<Later>, Stop> {
    if !valid_branch(&record.branch) || !is_id(&record.merge_commit) {
        return Err(Stop::Git);
    }
    budget.take()?;
    let ancestor = git_raw(
        repo,
        &[
            "merge-base",
            "--is-ancestor",
            &record.merge_commit,
            &record.branch,
        ],
    )
    .await
    .map_err(|_| Stop::Git)?;
    if ancestor.status.code() != Some(0) {
        return Err(Stop::Git);
    }
    let range = format!("{}..{}", record.merge_commit, record.branch);
    let count = format!("--max-count={COMMITS_PER_MERGE}");
    let out = budget
        .out(
            repo,
            &[
                "-c",
                "core.quotepath=false",
                "log",
                "--no-merges",
                "--name-only",
                &count,
                "--format=%x1e%H%x1f%ct%x1f%B%x1f",
                &range,
                "--",
            ],
        )
        .await?;
    Ok(parse_log(&out))
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
                && !c.files.is_empty()
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
                "-c",
                "core.quotepath=false",
                "diff",
                "--name-only",
                "--no-ext-diff",
                "--no-textconv",
                before,
                merge,
                "--",
            ],
        )
        .await?;
    let merged_files: BTreeSet<&str> = files_out.lines().take(MERGE_FILES_MAX).collect();
    let shared: Vec<(&Later, Vec<&str>)> = fixes
        .into_iter()
        .map(|c| {
            let both = c
                .files
                .iter()
                .map(String::as_str)
                .filter(|f| merged_files.contains(f))
                .take(FILES_PER_FIX)
                .collect::<Vec<_>>();
            (c, both)
        })
        .filter(|(_, both)| !both.is_empty())
        .collect();
    if shared.is_empty() {
        return Ok(None);
    }
    let range = format!("{before}..{merge}");
    let max = format!("--max-count={}", INTRODUCED_MAX + 1);
    let introduced: BTreeSet<String> = budget
        .out(repo, &["rev-list", &max, &range])
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
                        file,
                    ],
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
            let mut args: Vec<String> = vec!["blame".into(), "--porcelain".into()];
            for (from, to) in ranges {
                args.push("-L".into());
                args.push(format!("{from},{to}"));
            }
            args.extend([parent.clone(), "--".into(), file.to_owned()]);
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            match budget.out(repo, &refs).await {
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
        let candidates = self
            .store
            .crew_escape_scan_candidates(since, limits.merges + 1)?;
        let mut outcome = ScanOutcome {
            truncated: candidates.len() > limits.merges as usize,
            ..ScanOutcome::default()
        };
        let mut full: BTreeMap<String, bool> = BTreeMap::new();
        let mut budget = Budget {
            calls: limits.calls,
            deadline: Instant::now() + Duration::from_secs(SCAN_SECONDS),
        };
        let mut found: BTreeMap<String, Vec<PossibleEscape>> = BTreeMap::new();
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
                continue;
            }
            match look(&mut budget, record, project, thresholds.follow_up_days).await {
                Ok(Look::Found(item)) => found.entry(project.clone()).or_default().push(item),
                Ok(Look::Nothing) => {}
                Ok(Look::Unread) => outcome.skipped += 1,
                Err(_) => {
                    outcome.truncated = true;
                    break;
                }
            }
            outcome.scanned += 1;
        }
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
        Ok(outcome)
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
