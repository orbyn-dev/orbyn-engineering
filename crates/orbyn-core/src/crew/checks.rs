//! The check runners of the verifier (docs/crew.md, "Verification without limits"). Each runs
//! one check of the answer key in the task's checkout and says what happened. It is code, not a
//! model, and it never decides pass or fail for the whole key: `verify` does that.
//!
//! This version runs the command kinds (`command`, `test`, `lint`, `typecheck`,
//! `custom_script`), `file_scope`, `diff_size` and `schema`. Any other kind reports `skipped`, so
//! the result lists it as not verified rather than passing or failing on a guess.

use std::path::Path;
use std::time::{Duration, Instant};

use orbyn_contracts::{
    CheckOutcome, CheckParams, CrewCheck, CrewPath, CrewPathPattern, SchemaParams,
};
use tokio::process::Command;

use crate::error::CoreError;

/// No check runs longer than this.
const TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// The most a diff-size count reads of an untracked file.
const COUNTED_BYTES: u64 = 2 * 1024 * 1024;
/// The most instance locations a schema failure names.
const SCHEMA_NAMES: usize = 20;
/// Where a `schema` check's one file is registered while it is compiled. References to another
/// file or a URL cannot be resolved, so the check reads only the two files it was given.
const SCHEMA_LOC: &str = "https://orbyn.dev/crew/check/schema.json";

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
    /// What failed, by name: the failing tests, or the files outside the lease. The loop guard
    /// compares these, not the output, which changes between runs.
    pub names: Vec<String>,
}

/// Where the checks run.
pub struct Site<'a> {
    pub worktree: &'a Path,
    /// When the task's time budget ends: no check runs past it.
    pub deadline: Instant,
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
        names: Vec::new(),
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

/// The core's fix guard. Only the verifier of a recorded fix task calls this;
/// ordinary `repo_required` checks remain unimplemented.
pub(super) fn fix_touched(path: &str, digest: &str, site: &Site<'_>) -> Ran {
    let started = Instant::now();
    let description = "byte comparison: chk-fix-touched".to_owned();
    if Instant::now() >= site.deadline {
        return error(
            description,
            "The task's time budget ran out before this check could run.",
            started,
        );
    }
    let Ok(pattern) = CrewPathPattern::new(path) else {
        return error(description, "The finding's path is invalid.", started);
    };
    if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
        return error(
            description,
            "The fix's original bytes were not recorded.",
            started,
        );
    }
    let snapshot = super::protect::snapshot(site.worktree, &[pattern]);
    if Instant::now() >= site.deadline {
        return error(
            description,
            "The task's time budget ran out during the byte check.",
            started,
        );
    }
    let Some(now) = snapshot.get(path).filter(|entry| entry.kind == "file") else {
        return error(
            description,
            "The finding's file cannot be read as a regular file.",
            started,
        );
    };
    let mut result = ran(
        if now.sha256 == digest {
            CheckOutcome::Fail
        } else {
            CheckOutcome::Pass
        },
        description,
        String::new(),
        started,
    );
    if now.sha256 == digest {
        result.reason = Some("The finding's file has not changed.".to_owned());
        result.names.push(path.to_owned());
    }
    result
}

/// Runs one check.
pub async fn run_check(check: &CrewCheck, site: &Site<'_>) -> Ran {
    let started = Instant::now();
    let description = format!("{}: {}", kind_name(check), check.id);
    match check.typed_params() {
        Ok(CheckParams::Run(params)) => {
            run_command(&params.run, params.expect_exit.unwrap_or(0), site, started).await
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
        Ok(CheckParams::Schema(params)) => schema(&params, site, description, started),
        Ok(_) => skipped(check, started),
        Err(why) => error(description, &why.message, started),
    }
}

/// The result of a kind this version does not run: it says so, and it passes and fails nothing.
fn skipped(check: &CrewCheck, started: Instant) -> Ran {
    ran(
        CheckOutcome::Skipped,
        format!("{}: not run by this version", kind_name(check)),
        String::new(),
        started,
    )
}

/// Runs one command line in `worktree`, as the answer key's command checks run: it passes on exit 0.
pub(crate) async fn run_line(run: &str, worktree: &Path, deadline: Instant) -> Ran {
    let site = Site {
        worktree,
        deadline,
        base: None,
    };
    run_command(run, 0, &site, Instant::now()).await
}

async fn run_command(run: &str, expect: u32, site: &Site<'_>, started: Instant) -> Ran {
    let cwd = site.worktree;
    let allowed = TIMEOUT.min(site.deadline.saturating_duration_since(Instant::now()));
    if allowed.is_zero() {
        return error(
            run.to_owned(),
            "The task's time budget ran out before this check could run.",
            started,
        );
    }
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
    let output = match tokio::time::timeout(allowed, output).await {
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
        result.names = failing_names(&result.log);
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

fn failure_patterns() -> &'static [regex::Regex; 4] {
    static PATTERNS: std::sync::OnceLock<[regex::Regex; 4]> = std::sync::OnceLock::new();
    PATTERNS.get_or_init(|| {
        [
            r"(?m)^test (\S+) \.\.\. FAILED",
            r"(?m)^\s*--- FAIL: (\S+)",
            r"(?m)^\s*FAILED (\S+)",
            r"(?m)^\s*(?:\u{2715}|\u{2717}|\u{00d7}) (.+?)(?: \(\d+ ?ms\))?\s*$",
        ]
        .map(|p| regex::Regex::new(p).expect("a fixed pattern"))
    })
}

/// The names of the tests a log says failed, sorted and without repeats.
pub fn failing_names(log: &str) -> Vec<String> {
    let mut names: Vec<String> = failure_patterns()
        .iter()
        .flat_map(|p| p.captures_iter(log))
        .map(|c| c[1].to_owned())
        .collect();
    names.sort();
    names.dedup();
    names
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
pub(super) async fn changed(site: &Site<'_>) -> Result<Vec<String>, CoreError> {
    use crate::source_control::git::git_byte_exact as git;
    // GIT_ATTR_SOURCE does not replace info/attributes. Do not read its contents:
    // encoding and eol attributes there could normalize a byte change away.
    let location = git(
        site.worktree,
        &["rev-parse", "--git-path", "info/attributes"],
    )
    .await?;
    let location = std::str::from_utf8(&location)
        .map_err(|_| CoreError::Git("The Git attributes path is not valid UTF-8.".into()))?;
    let location = location.strip_suffix('\n').unwrap_or(location);
    let location = location.strip_suffix('\r').unwrap_or(location);
    let attributes = site.worktree.join(location);
    match std::fs::symlink_metadata(&attributes) {
        Err(why) if why.kind() == std::io::ErrorKind::NotFound => {}
        Ok(meta) if meta.is_file() && meta.len() == 0 => {
            std::fs::File::open(&attributes).map_err(|_| CoreError::Git("Git info/attributes cannot be inspected.".into()))?;
        }
        _ => return Err(CoreError::Git("Git info/attributes overrides must be absent or an empty readable regular file for Crew checks.".into())),
    }
    // A flagged entry can hide a tracked edit, including when the flag predates this task.
    match super::index_flags::flagged(site.worktree).await {
        Some(flags) if flags.is_empty() => {}
        _ => {
            return Err(CoreError::Git(
                "The checkout has an unreadable index or flags that hide changes.".into(),
            ));
        }
    }
    let base = site.base.unwrap_or("HEAD");
    let tracked = git(
        site.worktree,
        &[
            "diff",
            "--no-renames",
            "--no-ext-diff",
            "--name-only",
            "-z",
            base,
        ],
    )
    .await?;
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
        result.names = outside.iter().map(|f| (*f).clone()).collect();
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
        let numstat = crate::source_control::git::git_byte_exact(
            site.worktree,
            &[
                "diff",
                "--no-renames",
                "--no-ext-diff",
                "--numstat",
                "-z",
                base,
            ],
        )
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
        let fresh = crate::source_control::git::git_byte_exact(
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

/// A schema check reads one file: a reference to another file or a URL is refused rather than
/// resolved, so nothing outside the two files the key names is ever read.
struct NoExternal;

/// The first reference in a schema that is not an internal `#` one, if there is one. A schema
/// this check reads is one self-contained document; a reference to another file or URL is refused
/// before it is compiled, so the check never reads or fetches anything the key did not name.
fn external_reference(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => {
            for key in ["$ref", "$dynamicRef", "$recursiveRef"] {
                if let Some(text) = map.get(key).and_then(serde_json::Value::as_str)
                    && !text.starts_with('#')
                {
                    return Some(text.to_owned());
                }
            }
            map.values().find_map(external_reference)
        }
        serde_json::Value::Array(items) => items.iter().find_map(external_reference),
        _ => None,
    }
}

impl boon::UrlLoader for NoExternal {
    fn load(&self, _url: &str) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        Err("a schema check reads one self-contained schema file".into())
    }
}

/// Reads one regular file of the checkout, as the checks that name files read it. A symlink,
/// directory or file that cannot be read is an error, never an empty value.
fn checkout_file(site: &Site<'_>, path: &CrewPath, what: &str) -> Result<String, String> {
    let full = site.worktree.join(path.as_str());
    let meta = std::fs::symlink_metadata(&full)
        .map_err(|_| format!("the {what} file {} cannot be read", path.as_str()))?;
    if !meta.is_file() {
        return Err(format!(
            "the {what} file {} is not a regular file",
            path.as_str()
        ));
    }
    std::fs::read_to_string(&full)
        .map_err(|_| format!("the {what} file {} cannot be read", path.as_str()))
}

/// The instance locations a schema validation failed at, deduplicated and ordered.
fn schema_locations(why: &boon::ValidationError<'_, '_>) -> Vec<String> {
    let output = serde_json::to_value(why.basic_output()).unwrap_or(serde_json::Value::Null);
    let mut names: Vec<String> = Vec::new();
    let mut units = vec![&output];
    while let Some(unit) = units.pop() {
        if let Some(children) = unit["errors"].as_array() {
            units.extend(children);
        }
        let Some(at) = unit["instanceLocation"].as_str() else {
            continue;
        };
        let at = if at.is_empty() { "(root)" } else { at };
        if names.iter().all(|name| name != at) {
            names.push(at.to_owned());
        }
    }
    names.sort();
    names.truncate(SCHEMA_NAMES);
    names
}

/// Runs a `schema` check: the instance file, read from the checkout, must validate against the
/// schema file it names. Both are JSON, and the schema is one self-contained document. A mismatch
/// is a finding (fail); a file that cannot be read or a schema that cannot be compiled is an error.
fn schema(params: &SchemaParams, site: &Site<'_>, description: String, started: Instant) -> Ran {
    if Instant::now() >= site.deadline {
        return error(
            description,
            "The task's time budget ran out before this check could run.",
            started,
        );
    }
    let source = match checkout_file(site, &params.schema, "schema") {
        Ok(text) => text,
        Err(why) => return error(description, &why, started),
    };
    let schema: serde_json::Value = match serde_json::from_str(&source) {
        Ok(value) => value,
        Err(why) => {
            return error(
                description,
                &format!(
                    "the schema file {} is not valid JSON: {why}",
                    params.schema.as_str()
                ),
                started,
            );
        }
    };
    if let Some(reference) = external_reference(&schema) {
        return error(
            description,
            &format!(
                "the schema file {} refers to {reference}; a schema check reads one self-contained schema file",
                params.schema.as_str()
            ),
            started,
        );
    }
    let instance = match checkout_file(site, &params.instance, "instance") {
        Ok(text) => text,
        Err(why) => return error(description, &why, started),
    };
    let instance: serde_json::Value = match serde_json::from_str(&instance) {
        Ok(value) => value,
        Err(why) => {
            let mut result = ran(
                CheckOutcome::Fail,
                description,
                format!("{} is not valid JSON: {why}\n", params.instance.as_str()),
                started,
            );
            result.names.push(params.instance.as_str().to_owned());
            result.reason = Some(format!(
                "The instance {} is not valid JSON: {why}",
                params.instance.as_str()
            ));
            return result;
        }
    };
    let mut compiler = boon::Compiler::new();
    compiler.set_default_draft(boon::Draft::V2020_12);
    compiler.use_loader(Box::new(NoExternal));
    if let Err(why) = compiler.add_resource(SCHEMA_LOC, schema) {
        return error(
            description,
            &format!(
                "the schema file {} cannot be read: {why}",
                params.schema.as_str()
            ),
            started,
        );
    }
    let mut schemas = boon::Schemas::new();
    let index = match compiler.compile(SCHEMA_LOC, &mut schemas) {
        Ok(index) => index,
        Err(why) => {
            return error(
                description,
                &format!(
                    "the schema file {} is not a JSON Schema this version compiles: {why}",
                    params.schema.as_str()
                ),
                started,
            );
        }
    };
    match schemas.validate(&instance, index) {
        Ok(()) => ran(
            CheckOutcome::Pass,
            description,
            format!(
                "{} is valid against {}\n",
                params.instance.as_str(),
                params.schema.as_str()
            ),
            started,
        ),
        Err(why) => {
            let names = schema_locations(&why);
            let found = if names.is_empty() {
                "(no location reported)".to_owned()
            } else {
                names.join(", ")
            };
            let mut result = ran(
                CheckOutcome::Fail,
                description,
                format!(
                    "{} does not match {}: {found}\n",
                    params.instance.as_str(),
                    params.schema.as_str()
                ),
                started,
            );
            result.reason = Some(format!(
                "The instance {} does not match the schema at: {found}.",
                params.instance.as_str()
            ));
            result.names = names;
            result
        }
    }
}
