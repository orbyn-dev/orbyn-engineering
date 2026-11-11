//! The check runners of the verifier (docs/crew.md, "Verification without limits"). Each runs
//! one check of the answer key in the task's checkout and says what happened. It is code, not a
//! model, and it never decides pass or fail for the whole key: `verify` does that.
//!
//! This version runs the command kinds (`command`, `test`, `lint`, `typecheck`,
//! `custom_script`), `file_scope` and `diff_size`. Any other kind reports `skipped`, so the
//! result lists it as not verified rather than passing or failing on a guess.

use std::path::Path;
use std::time::{Duration, Instant};

use orbyn_contracts::{CheckOutcome, CheckParams, CrewCheck, CrewPathPattern};
use tokio::process::Command;

use crate::error::CoreError;

/// No check runs longer than this.
const TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// The most a diff-size count reads of an untracked file.
const COUNTED_BYTES: u64 = 2 * 1024 * 1024;

/// What one check did.
#[derive(Clone, Debug)]
pub struct Ran {
    pub outcome: CheckOutcome,
    pub exit_code: Option<u32>,
    /// The exact command, or the check's own description when nothing ran.
    pub command: String,
    pub log: String,
    pub duration_ms: u64,
    /// Why it failed, in words a worker may read. It never holds the check itself.
    pub reason: Option<String>,
}

/// Where the checks run.
pub struct Site<'a> {
    pub worktree: &'a Path,
    /// The commit the worker started from, for the diff checks.
    pub base: Option<&'a str>,
}

fn ran(outcome: CheckOutcome, command: String, log: String, started: Instant) -> Ran {
    Ran {
        outcome,
        exit_code: None,
        command,
        log,
        duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        reason: None,
    }
}

fn error(command: String, why: &str, started: Instant) -> Ran {
    let mut result = ran(CheckOutcome::Error, command, why.to_owned(), started);
    result.reason = Some(why.to_owned());
    result
}

fn kind_name(check: &CrewCheck) -> String {
    super::wire_name(&check.kind)
}

/// Runs one check.
pub async fn run_check(check: &CrewCheck, site: &Site<'_>) -> Ran {
    let started = Instant::now();
    let description = format!("{}: {}", kind_name(check), check.id);
    match check.typed_params() {
        Ok(CheckParams::Run(params)) => {
            run_command(
                &params.run,
                params.expect_exit.unwrap_or(0),
                site.worktree,
                started,
            )
            .await
        }
        Ok(CheckParams::FileScope(params)) => {
            file_scope(&params.only, site, description, started).await
        }
        Ok(CheckParams::DiffSize(params)) => {
            diff_size(
                params.max_files,
                params.max_lines,
                site,
                description,
                started,
            )
            .await
        }
        Ok(_) => ran(
            CheckOutcome::Skipped,
            format!("{}: not run by this version", kind_name(check)),
            String::new(),
            started,
        ),
        Err(why) => error(description, &why.message, started),
    }
}

async fn run_command(run: &str, expect: u32, cwd: &Path, started: Instant) -> Ran {
    let (program, args) = crate::host::terminal::shell_command(run);
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    crate::host::child_env::for_tool(command.as_std_mut());
    let output = async {
        crate::host::process::ManagedChild::spawn(command)?
            .output()
            .await
    };
    let output = match tokio::time::timeout(TIMEOUT, output).await {
        Ok(Ok(output)) => output,
        Ok(Err(why)) => {
            return error(
                run.to_owned(),
                &format!("The check could not start: {}", why.public_message()),
                started,
            );
        }
        Err(_) => {
            return error(
                run.to_owned(),
                "The check ran out of time and was stopped.",
                started,
            );
        }
    };
    let mut log = String::from_utf8_lossy(&output.stdout).into_owned();
    let errors = String::from_utf8_lossy(&output.stderr);
    if !errors.is_empty() {
        log.push_str(&errors);
    }
    let code = output.status.code().and_then(|c| u32::try_from(c).ok());
    let passed = code == Some(expect);
    let mut result = ran(
        if passed {
            CheckOutcome::Pass
        } else {
            CheckOutcome::Fail
        },
        run.to_owned(),
        log,
        started,
    );
    result.exit_code = code;
    if !passed {
        let ended = code.map_or_else(
            || "was stopped by a signal".to_owned(),
            |c| format!("exited with {c}"),
        );
        result.reason = Some(format!(
            "The check {ended}, and it should exit with {expect}.\n{}",
            tail(&result.log, 1500)
        ));
    }
    result
}

/// The last `max` characters of `text`, cut at a line.
pub fn tail(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.trim().to_owned();
    }
    let cut: String = text.chars().skip(count - max).collect();
    let cut = cut.split_once('\n').map_or(cut.as_str(), |(_, rest)| rest);
    cut.trim().to_owned()
}

/// Whether `path` is inside the pattern: the same file, or under a `dir/**` tree.
pub fn covers(pattern: &CrewPathPattern, path: &str) -> bool {
    CrewPathPattern::new(path).is_ok_and(|file| pattern.overlaps(&file))
}

/// Every path that differs from the starting commit: changed tracked files and new ones.
async fn changed(site: &Site<'_>) -> Result<Vec<String>, CoreError> {
    use crate::source_control::git::git;
    let base = site.base.unwrap_or("HEAD");
    let tracked = git(site.worktree, &["diff", "--name-only", "-z", base]).await?;
    let fresh = git(
        site.worktree,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )
    .await?;
    let mut paths: Vec<String> = tracked
        .split(|b| *b == 0)
        .chain(fresh.split(|b| *b == 0))
        .filter(|name| !name.is_empty())
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

async fn file_scope(
    only: &[CrewPathPattern],
    site: &Site<'_>,
    description: String,
    started: Instant,
) -> Ran {
    let files = match changed(site).await {
        Ok(files) => files,
        Err(why) => return error(description, &why.public_message(), started),
    };
    let outside: Vec<&String> = files
        .iter()
        .filter(|file| !only.iter().any(|p| covers(p, file)))
        .collect();
    let log = format!("{} changed, {} outside\n", files.len(), outside.len());
    let mut result = ran(
        if outside.is_empty() {
            CheckOutcome::Pass
        } else {
            CheckOutcome::Fail
        },
        description,
        log,
        started,
    );
    if !outside.is_empty() {
        let shown: Vec<&str> = outside.iter().take(10).map(|f| f.as_str()).collect();
        let more = outside.len().saturating_sub(shown.len());
        result.reason = Some(format!(
            "Files changed outside the paths you were given: {}{}.",
            shown.join(", "),
            if more > 0 {
                format!(" and {more} more")
            } else {
                String::new()
            }
        ));
    }
    result
}

async fn diff_size(
    max_files: Option<u64>,
    max_lines: Option<u64>,
    site: &Site<'_>,
    description: String,
    started: Instant,
) -> Ran {
    let counted = async {
        let files = changed(site).await?;
        let base = site.base.unwrap_or("HEAD");
        let numstat =
            crate::source_control::git::git(site.worktree, &["diff", "--numstat", "-z", base])
                .await?;
        let text = String::from_utf8_lossy(&numstat).into_owned();
        let mut lines: u64 = text
            .split('\0')
            .filter_map(|row| {
                let mut cells = row.split('\t');
                let added = cells.next()?.trim().parse::<u64>().ok()?;
                let removed = cells.next()?.trim().parse::<u64>().ok()?;
                Some(added + removed)
            })
            .sum();
        let fresh = crate::source_control::git::git(
            site.worktree,
            &["ls-files", "--others", "--exclude-standard", "-z"],
        )
        .await?;
        for name in fresh.split(|b| *b == 0).filter(|n| !n.is_empty()) {
            let path = site.worktree.join(String::from_utf8_lossy(name).as_ref());
            if std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= COUNTED_BYTES)
                && let Ok(bytes) = std::fs::read(&path)
            {
                lines += bytes.iter().filter(|b| **b == b'\n').count() as u64;
            }
        }
        Ok::<_, CoreError>((files.len() as u64, lines))
    };
    let (files, lines) = match counted.await {
        Ok(counts) => counts,
        Err(why) => return error(description, &why.public_message(), started),
    };
    let too_many = max_files.is_some_and(|max| files > max);
    let too_long = max_lines.is_some_and(|max| lines > max);
    let mut result = ran(
        if too_many || too_long {
            CheckOutcome::Fail
        } else {
            CheckOutcome::Pass
        },
        description,
        format!("{files} files, {lines} lines\n"),
        started,
    );
    if too_many || too_long {
        result.reason = Some(format!(
            "The change is larger than allowed: {files} files and {lines} lines."
        ));
    }
    result
}
