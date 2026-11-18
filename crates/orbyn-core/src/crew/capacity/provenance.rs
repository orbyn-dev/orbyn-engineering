//! Per-hunk provenance recorded at merge: which new-file line ranges the merge brought in, who wrote each (the
//! task's engine and model, or a person who committed on the change) and a local git note that carries the same
//! ids. Parsing git's diff text is pure and separate from running git. Ids, ranges and counts only: never file
//! content, a prompt, a secret or a path outside the repository.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use serde_json::{Value, json};

use crate::agents::driver::worker_identity::{TASK_TRAILER, is_task_id, task_of_address};
use crate::error::{CoreError, Result};
use crate::source_control::git::{git, git_byte_exact, text};
use crate::store::{MergeRecord, Store};

/// The ref the note lives under. It is local only: the app never pushes or fetches it.
pub const NOTE_REF: &str = "refs/notes/orbyn-agent-trace";

/// The identity the note's own commit is made with, so a missing git identity cannot stop it.
const NOTE_IDENTITY: [&str; 4] = [
    "-c",
    "user.name=Orbyn Crew",
    "-c",
    "user.email=crew@orbyn.invalid",
];

/// The address of the app's own commits (merges, notes, reverts). Such a commit is never a worker's and never a person's.
const APP_EMAIL: &str = "crew@orbyn.invalid";

/// A run of lines in the merged (new) file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    /// Repository-relative, with `/` separators.
    pub path: String,
    /// First line, counting from 1.
    pub start: u32,
    /// Number of lines, at least 1.
    pub len: u32,
}

/// Who wrote a hunk.
///
/// `Agent` is positive evidence: the commit's author or committer is a worker address of an existing task, or its
/// message carries that task's trailer. `Person` is positive evidence too: the commit id is in the project's recorded
/// person commits. `Unknown` is everything else (a person's manual commit, history from before worker identities, a
/// bot, the app's own conflict resolution). It carries the merge task's engine and model so coverage can count it
/// with the agent's lines: an unconfirmed line is never credited as human-written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Author {
    Agent {
        engine: String,
        model: Option<String>,
    },
    Person,
    Unknown {
        engine: String,
        model: Option<String>,
    },
}

impl Author {
    /// The same engine and model as an `Unknown` author: what a line nobody confirmed reads as.
    fn unconfirmed(&self) -> Author {
        match self {
            Author::Agent { engine, model } | Author::Unknown { engine, model } => {
                Author::Unknown {
                    engine: engine.clone(),
                    model: model.clone(),
                }
            }
            Author::Person => Author::Person,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthoredHunk {
    pub hunk: Hunk,
    pub author: Author,
}

/// What `record_at_merge` did. The store write succeeded, or the call returned an error; the note may still have failed.
#[derive(Debug)]
pub struct Recorded {
    pub hunks: usize,
    /// Why the git note was not written, for the caller to log. The merge path never fails on it.
    pub note: std::result::Result<(), CoreError>,
}

// ---- pure: diff text ----

/// A path git printed, made safe to store and show: repository-relative, `/`-separated, no `.` or `..` component, no
/// drive, no control character. Anything else is dropped rather than rendered.
fn safe_path(path: &str) -> Option<String> {
    let bad = path.is_empty()
        || path.starts_with('/')
        || path.chars().any(|c| c.is_control())
        || path.as_bytes().get(1) == Some(&b':')
        || path.contains('\\')
        || path.split('/').any(|part| matches!(part, "" | "." | ".."));
    (!bad).then(|| path.to_owned())
}

/// Undoes git's C-style quoting of a path (`"a\tb"`, octal bytes).
fn unquote(text: &[u8]) -> Option<Vec<u8>> {
    let inner = text.strip_prefix(b"\"")?.strip_suffix(b"\"")?;
    let mut out = Vec::with_capacity(inner.len());
    let mut bytes = inner.iter().copied();
    while let Some(byte) = bytes.next() {
        if byte != b'\\' {
            out.push(byte);
            continue;
        }
        out.push(match bytes.next()? {
            b'a' => 7,
            b'b' => 8,
            b'f' => 12,
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            b'v' => 11,
            b'\\' => b'\\',
            b'"' => b'"',
            digit @ b'0'..=b'3' => {
                let (b, c) = (bytes.next()?, bytes.next()?);
                if !(b'0'..=b'7').contains(&b) || !(b'0'..=b'7').contains(&c) {
                    return None;
                }
                ((digit - b'0') << 6) | ((b - b'0') << 3) | (c - b'0')
            }
            _ => return None,
        });
    }
    Some(out)
}

/// The path of a `+++ ` header: `None` for a deleted file (`/dev/null`) or a path that is not safe.
fn header_path(rest: &[u8]) -> Option<String> {
    let rest = match rest.iter().position(|b| *b == b'\t') {
        // Git appends a tab after a name that holds a space.
        Some(at) if !rest.starts_with(b"\"") => &rest[..at],
        _ => rest,
    };
    if rest == b"/dev/null" {
        return None;
    }
    let raw = if rest.starts_with(b"\"") {
        unquote(rest)?
    } else {
        rest.to_vec()
    };
    let text = String::from_utf8(raw).ok()?;
    safe_path(text.strip_prefix("b/")?)
}

/// `(old_len, new_start, new_len)` of a `@@ -a[,b] +c[,d] @@` header, without the leading `@@ `.
fn range_of(header: &[u8]) -> Option<(u32, u32, u32)> {
    let text = std::str::from_utf8(header).ok()?;
    let mut parts = text.split(' ');
    let old = parts.next()?.strip_prefix('-')?;
    let new = parts.next()?.strip_prefix('+')?;
    let pair = |part: &str| -> Option<(u32, u32)> {
        let (start, len) = part.split_once(',').unwrap_or((part, "1"));
        Some((start.parse().ok()?, len.parse().ok()?))
    };
    let (_, old_len) = pair(old)?;
    let (start, len) = pair(new)?;
    Some((old_len, start, len))
}

/// The new-file line ranges of a `git diff -U0` (or `git show -U0`) text. Added and modified files give their changed
/// ranges; a rename gives only what changed beyond the rename; a deleted file, a pure deletion and a binary file give
/// nothing. The hunk bodies are counted, so a line of content that looks like a header cannot start a file.
pub fn parse_diff(diff: &[u8]) -> Vec<Hunk> {
    let mut hunks = Vec::new();
    let mut path: Option<String> = None;
    let (mut old_left, mut new_left) = (0u32, 0u32);
    for line in diff.split(|b| *b == b'\n') {
        if old_left > 0 || new_left > 0 {
            match line.first() {
                Some(b'-') => old_left = old_left.saturating_sub(1),
                Some(b'+') => new_left = new_left.saturating_sub(1),
                Some(b' ') => {
                    old_left = old_left.saturating_sub(1);
                    new_left = new_left.saturating_sub(1);
                }
                _ => {}
            }
            continue;
        }
        if line.starts_with(b"diff --git ")
            || line.starts_with(b"Binary files ")
            || line.starts_with(b"GIT binary patch")
        {
            path = None;
        } else if let Some(rest) = line.strip_prefix(b"+++ ") {
            path = header_path(rest);
        } else if let Some(rest) = line.strip_prefix(b"@@ ") {
            let Some((old_len, start, len)) = range_of(rest) else {
                continue;
            };
            (old_left, new_left) = (old_len, len);
            if let (Some(path), true) = (&path, len > 0 && start > 0) {
                hunks.push(Hunk {
                    path: path.clone(),
                    start,
                    len,
                });
            }
        }
    }
    hunks
}

/// The line to commit map of `git blame --porcelain` output: each result line (final line number) and the full
/// commit id that last changed it. Content lines start with a tab and are never read.
pub fn parse_blame(blame: &[u8]) -> BTreeMap<u32, String> {
    let mut lines = BTreeMap::new();
    for line in blame.split(|b| *b == b'\n') {
        if line.first() == Some(&b'\t') {
            continue;
        }
        let Ok(text) = std::str::from_utf8(line) else {
            continue;
        };
        let mut parts = text.split(' ');
        let (Some(id), Some(_orig), Some(fin)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        if is_revision(id)
            && let Ok(fin) = fin.parse::<u32>()
        {
            lines.insert(fin, id.to_ascii_lowercase());
        }
    }
    lines
}

/// The author and committer email of one commit, as `git blame --porcelain` printed them, without the angle brackets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommitMails {
    pub author: Option<String>,
    pub committer: Option<String>,
}

impl CommitMails {
    fn is_app(&self) -> bool {
        [&self.author, &self.committer]
            .into_iter()
            .flatten()
            .any(|mail| mail == APP_EMAIL)
    }
}

/// The emails of each commit in `git blame --porcelain` output. Git prints a commit's metadata once, after the first
/// header line that names it; content lines start with a tab and are never read, so file text cannot forge a row.
/// These are read only to find a worker's address: an email can make a line `agent` or leave it `unknown`, and can
/// never make it a person's.
pub fn parse_blame_mails(blame: &[u8]) -> HashMap<String, CommitMails> {
    let mut mails: HashMap<String, CommitMails> = HashMap::new();
    let mut current: Option<String> = None;
    for line in blame.split(|b| *b == b'\n') {
        if line.first() == Some(&b'\t') {
            continue;
        }
        let Ok(text) = std::str::from_utf8(line) else {
            continue;
        };
        let mut parts = text.split(' ');
        if let (Some(id), Some(_), Some(fin)) = (parts.next(), parts.next(), parts.next())
            && is_revision(id)
            && fin.parse::<u32>().is_ok()
        {
            let id = id.to_ascii_lowercase();
            mails.entry(id.clone()).or_default();
            current = Some(id);
            continue;
        }
        let Some(id) = &current else {
            continue;
        };
        let strip = |rest: &str| {
            rest.trim()
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_owned()
        };
        if let Some(rest) = text.strip_prefix("author-mail ") {
            mails.entry(id.clone()).or_default().author = Some(strip(rest));
        } else if let Some(rest) = text.strip_prefix("committer-mail ") {
            mails.entry(id.clone()).or_default().committer = Some(strip(rest));
        }
    }
    mails
}

/// Whether either email of a commit is the address of a worker of an existing task.
fn worker_mail(mails: &CommitMails, known_task: &dyn Fn(&str) -> bool) -> bool {
    [&mails.author, &mails.committer]
        .into_iter()
        .flatten()
        .filter_map(|mail| task_of_address(mail))
        .any(known_task)
}

/// Splits one merged hunk into contiguous runs of the same author. A line whose commit is in `agent_commits` is the
/// agent's, whatever else is said of it; otherwise it is a person's only if blame names a commit in `person`;
/// otherwise it is `Unknown`, including a line blame did not name. `agent` gives the engine and model.
pub fn split_by_author(
    hunk: &Hunk,
    blame: &BTreeMap<u32, String>,
    person: &HashSet<String>,
    agent_commits: &HashSet<String>,
    agent: &Author,
) -> Vec<AuthoredHunk> {
    let mut out: Vec<AuthoredHunk> = Vec::new();
    for line in hunk.start..hunk.start + hunk.len {
        let commit = blame.get(&line);
        let author = match commit {
            Some(id) if agent_commits.contains(id) => agent.clone(),
            Some(id) if person.contains(id) => Author::Person,
            _ => agent.unconfirmed(),
        };
        match out.last_mut() {
            Some(last) if last.author == author => last.hunk.len += 1,
            _ => out.push(AuthoredHunk {
                hunk: Hunk {
                    path: hunk.path.clone(),
                    start: line,
                    len: 1,
                },
                author,
            }),
        }
    }
    out
}

/// The Agent Trace-shaped note body: version, the revision, and per file the ranges by contributor. Contributor
/// `type` is `agent`, `person` or `unknown`, with the engine and model ids for an agent. Nothing else goes in.
pub fn trace_note(merge_commit: &str, hunks: &[AuthoredHunk], now: i64) -> Value {
    let mut paths: Vec<&str> = hunks.iter().map(|h| h.hunk.path.as_str()).collect();
    paths.sort_unstable();
    paths.dedup();
    let files: Vec<Value> = paths
        .into_iter()
        .map(|path| {
            let mut contributors: Vec<&Author> = Vec::new();
            for item in hunks.iter().filter(|h| h.hunk.path == path) {
                if !contributors.contains(&&item.author) {
                    contributors.push(&item.author);
                }
            }
            let conversations: Vec<Value> = contributors
                .into_iter()
                .map(|author| {
                    let contributor = match author {
                        Author::Agent { engine, model } => {
                            json!({ "type": "agent", "engine": engine, "model": model })
                        }
                        Author::Person => json!({ "type": "person" }),
                        Author::Unknown { .. } => json!({ "type": "unknown" }),
                    };
                    let ranges: Vec<Value> = hunks
                        .iter()
                        .filter(|h| h.hunk.path == path && &h.author == author)
                        .map(|h| {
                            json!({
                                "start_line": h.hunk.start,
                                "end_line": h.hunk.start + h.hunk.len - 1,
                            })
                        })
                        .collect();
                    json!({ "contributor": contributor, "ranges": ranges })
                })
                .collect();
            json!({ "path": path, "conversations": conversations })
        })
        .collect();
    json!({
        "version": "0.1.0",
        "timestamp": now,
        "vcs": { "type": "git", "revision": merge_commit },
        "counts": {
            "files": files.len(),
            "hunks": hunks.len(),
            "lines": hunks.iter().map(|h| u64::from(h.hunk.len)).sum::<u64>(),
        },
        "files": files,
    })
}

// ---- git ----

fn is_revision(text: &str) -> bool {
    matches!(text.len(), 40 | 64) && text.bytes().all(|b| b.is_ascii_hexdigit())
}

fn need_revision(text: &str) -> Result<()> {
    if is_revision(text) {
        Ok(())
    } else {
        Err(CoreError::Invalid(
            "provenance needs full commit ids".into(),
        ))
    }
}

/// The byte-exact `-U0` diff arguments: no renames guessed beyond `-M`, no external or text-converting diff, fixed
/// prefixes so a repository's `diff.noprefix` cannot change what is parsed.
const DIFF_ARGS: [&str; 6] = [
    "-U0",
    "-M",
    "--no-color",
    "--no-ext-diff",
    "--src-prefix=a/",
    "--dst-prefix=b/",
];

/// The new-file hunks the merge brought in over `before`.
pub async fn hunks_of(repo: &Path, before: &str, merge: &str) -> Result<Vec<Hunk>> {
    need_revision(before)?;
    need_revision(merge)?;
    let mut args = vec!["diff"];
    args.extend(DIFF_ARGS);
    args.extend([before, merge, "--"]);
    Ok(parse_diff(&git_byte_exact(repo, &args).await?))
}

/// The ids positively known to be a person's, as a lowercase set. Anything that is not a full hex id is dropped, so
/// it can never match a blamed commit.
fn person_set(person_commits: &[String]) -> HashSet<String> {
    person_commits
        .iter()
        .map(|id| id.trim().to_ascii_lowercase())
        .filter(|id| is_revision(id))
        .collect()
}

/// The most commits whose message is read for a task trailer in one merge, and how many go in one `git log`. A commit
/// past the cap is not read and stays `unknown`, so a huge history cannot grow the work without bound.
const TRAILER_COMMITS_MAX: usize = 1024;
const TRAILER_CHUNK: usize = 64;

/// The task ids named by `Orbyn-Task` trailers, by commit, for the commits given (full ids). A commit whose message
/// cannot be read has no entry. Trailer text is whatever the committer wrote: the caller checks each id exists.
async fn trailer_tasks(repo: &Path, commits: &[String]) -> HashMap<String, Vec<String>> {
    let mut found = HashMap::new();
    let format = format!(
        "--format=%H%x1f%(trailers:key={},valueonly,unfold,separator=%x2C)%x1e",
        TASK_TRAILER
    );
    for chunk in commits
        .iter()
        .take(TRAILER_COMMITS_MAX)
        .collect::<Vec<_>>()
        .chunks(TRAILER_CHUNK)
    {
        let mut args: Vec<&str> = vec![
            "-c",
            "log.showSignature=false",
            "log",
            "--no-walk=unsorted",
            "--no-color",
            &format,
        ];
        args.extend(chunk.iter().map(|id| id.as_str()));
        args.push("--");
        let Ok(out) = git_byte_exact(repo, &args).await else {
            continue;
        };
        for record in String::from_utf8_lossy(&out).split('\u{1e}') {
            let Some((id, values)) = record.trim().split_once('\u{1f}') else {
                continue;
            };
            let id = id.trim().to_ascii_lowercase();
            if is_revision(&id) {
                let tasks = values
                    .split(',')
                    .map(|value| value.trim().to_owned())
                    .filter(|value| !value.is_empty())
                    .collect();
                found.insert(id, tasks);
            }
        }
    }
    found
}

/// Gives each hunk its author by `git blame` of the merge commit. A line is the agent's when the commit that last
/// changed it has a worker address (author or committer) of an existing task, or an `Orbyn-Task` trailer naming one
/// (`known_task` says whether a task exists). It is a person's only when that commit is one of `person`, and the
/// agent's mark wins if a commit has both. Anything else is `Unknown`: an unlisted commit, a rewritten id, the app's
/// own conflict resolution, a line blame did not name and a file whose blame failed. Reading an email can only give
/// `agent` or leave `unknown`. One blame runs per file, without `-L`, and the result is sliced, so the number of
/// hunks cannot grow the command line. Repository config cannot reattribute lines: the ignore-revs list is cleared. A
/// `.mailmap` can rewrite an email git prints; that can only hide a worker address (unknown) or invent one (agent),
/// never produce a person.
pub(crate) async fn attribute_by_blame(
    repo: &Path,
    merge: &str,
    hunks: Vec<Hunk>,
    person: &HashSet<String>,
    agent: &Author,
    known_task: &(dyn Fn(&str) -> bool + Sync),
) -> Vec<AuthoredHunk> {
    let mut paths: Vec<&str> = hunks.iter().map(|h| h.path.as_str()).collect();
    paths.sort_unstable();
    paths.dedup();
    let mut blamed: BTreeMap<String, BTreeMap<u32, String>> = BTreeMap::new();
    let mut mails: HashMap<String, CommitMails> = HashMap::new();
    for path in paths {
        let args = [
            "-c",
            "blame.ignoreRevsFile=",
            "blame",
            "--porcelain",
            "--ignore-revs-file",
            "",
            merge,
            "--",
            path,
        ];
        if let Ok(out) = git_byte_exact(repo, &args).await {
            blamed.insert(path.to_owned(), parse_blame(&out));
            for (id, found) in parse_blame_mails(&out) {
                mails.entry(id).or_insert(found);
            }
        }
    }
    let mut agent_commits: HashSet<String> = mails
        .iter()
        .filter(|(_, found)| worker_mail(found, known_task))
        .map(|(id, _)| id.clone())
        .collect();
    // The trailer corroborates the identity. Not read for a commit already marked, a person's, or the app's own.
    let mut unread: Vec<String> = mails
        .iter()
        .filter(|(id, found)| {
            !agent_commits.contains(*id) && !person.contains(*id) && !found.is_app()
        })
        .map(|(id, _)| id.clone())
        .collect();
    unread.sort_unstable();
    for (id, tasks) in trailer_tasks(repo, &unread).await {
        if tasks
            .iter()
            .any(|task| is_task_id(task) && known_task(task))
        {
            agent_commits.insert(id);
        }
    }
    let none = BTreeMap::new();
    hunks
        .iter()
        .flat_map(|hunk| {
            split_by_author(
                hunk,
                blamed.get(&hunk.path).unwrap_or(&none),
                person,
                &agent_commits,
                agent,
            )
        })
        .collect()
}

/// A scratch file inside the repository's own git directory, with a name nobody can predict, created exclusively
/// and removed when this is dropped, whether the note was written or not.
struct Scratch(std::path::PathBuf);

impl Scratch {
    async fn create(repo: &Path, body: &[u8]) -> Result<Self> {
        use std::io::Write;
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let dir = text(&git(repo, &["rev-parse", "--absolute-git-dir"]).await?);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        for _ in 0..8 {
            let name = format!(
                "orbyn-note-{}-{nanos:x}-{}.tmp",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            );
            let path = Path::new(&dir).join(name);
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => {
                    let scratch = Self(path);
                    file.write_all(body)?;
                    return Ok(scratch);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
        Err(CoreError::Internal("no free scratch name".into()))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// One lock per repository, so two merges recorded at once on it write the notes ref one after the other: each
/// `git notes add` reads the ref, makes a commit on it and moves it, and two at once can lose a note or hit a ref lock.
/// The key is the shared git directory (`--git-common-dir`), where the notes ref lives, so a repository and its linked
/// worktrees share one lock. The map grows by one entry per repository this process records for.
async fn note_lock(repo: &Path) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    use std::sync::{Arc, LazyLock, Mutex};
    static LOCKS: LazyLock<Mutex<HashMap<std::path::PathBuf, Arc<tokio::sync::Mutex<()>>>>> =
        LazyLock::new(Mutex::default);
    let common = git(repo, &["rev-parse", "--git-common-dir"])
        .await
        .map(|out| std::path::PathBuf::from(text(&out)))
        .map(|dir| {
            if dir.is_absolute() {
                dir
            } else {
                repo.join(dir)
            }
        })
        .unwrap_or_else(|_| repo.to_path_buf());
    let key = std::fs::canonicalize(&common).unwrap_or(common);
    let mut locks = LOCKS.lock().unwrap_or_else(|e| e.into_inner());
    Arc::clone(locks.entry(key).or_default())
}

/// The longest one `git notes add` may take while it holds the repository's lock, so one hung git cannot block every
/// later note write. A timeout is an ordinary failed note: a `provenance_failed` row, retried at the next merge.
const NOTE_GIT_LIMIT: std::time::Duration = std::time::Duration::from_secs(30);

/// How often one note write is tried when git reports a ref lock clash (another process, such as a person's own
/// git command, moving the same ref). The lock above already orders this process's own writes.
const NOTE_TRIES: u32 = 5;

fn is_lock_clash(error: &CoreError) -> bool {
    matches!(error, CoreError::Git(text) if text.contains("cannot lock ref") || text.contains(".lock"))
}

/// Writes the note on the merge commit, replacing an earlier one. Local only. The body goes through a scratch file
/// (`-F`), not the command line, so its size cannot hit a command-line limit. Writes on one repository are taken one at
/// a time, and a ref lock clash with another process is tried again a bounded number of times. Each git call is bounded by `NOTE_GIT_LIMIT`.
async fn write_note(repo: &Path, merge: &str, body: &Value) -> Result<()> {
    need_revision(merge)?;
    let scratch = Scratch::create(repo, &serde_json::to_vec(body)?).await?;
    let file_text = scratch.0.to_string_lossy().into_owned();
    let notes_ref = format!("--ref={NOTE_REF}");
    let mut args: Vec<&str> = NOTE_IDENTITY.to_vec();
    args.extend(["notes", &notes_ref, "add", "-f", "-F", &file_text, merge]);
    let lock = note_lock(repo).await;
    let _held = lock.lock().await;
    let mut tries = 1;
    loop {
        let attempt = tokio::time::timeout(NOTE_GIT_LIMIT, git(repo, &args))
            .await
            .unwrap_or_else(|_| Err(CoreError::Git("git notes timed out".into())));
        match attempt {
            Ok(_) => return Ok(()),
            Err(error) if tries < NOTE_TRIES && is_lock_clash(&error) => {
                tokio::time::sleep(std::time::Duration::from_millis(25 * u64::from(tries))).await;
                tries += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

/// Records a merge's hunks and its local git note. Hunks come from the diff of `before..merge`.
///
/// `person_commits` are full commit ids positively known to be a person's (the project's `crew_person_commits`).
/// `git blame` of the merge commit gives each line to the commit that last changed it. A line is the agent's when
/// that commit's author or committer email is a worker address of an existing task, or its message has that task's
/// `Orbyn-Task` trailer (see `attribute_by_blame`); a person's only if the commit is in the list; otherwise
/// `Unknown`, which coverage counts with the agent's lines. Emails are read to find worker addresses and can never
/// make a line a person's. The caller must pass only commits it positively identified, never a guess.
///
/// The store write is one transaction and its error is returned. The note is written after it, and a failure there
/// is returned inside `Recorded` for the caller to log, because the merge has already happened. A merge with no hunk
/// gets no note and no `note_ref`.
pub async fn record_at_merge(
    store: &Store,
    merge: &MergeRecord,
    agent: &Author,
    person_commits: &[String],
    now: i64,
) -> Result<Recorded> {
    let repo = Path::new(&merge.repo);
    let hunks = hunks_of(repo, &merge.before_commit, &merge.merge_commit).await?;
    let person = person_set(person_commits);
    let known = |id: &str| store.crew_task(id).is_ok_and(|task| task.is_some());
    let authored =
        attribute_by_blame(repo, &merge.merge_commit, hunks, &person, agent, &known).await;
    store.record_crew_hunks(merge.id, &merge.repo, now, &authored)?;
    if authored.is_empty() {
        return Ok(Recorded {
            hunks: 0,
            note: Ok(()),
        });
    }
    let body = trace_note(&merge.merge_commit, &authored, now);
    let note = match write_note(repo, &merge.merge_commit, &body).await {
        Ok(()) => store.set_crew_hunks_note_ref(merge.id, NOTE_REF),
        Err(error) => Err(error),
    };
    Ok(Recorded {
        hunks: authored.len(),
        note,
    })
}

#[cfg(test)]
#[path = "provenance_tests.rs"]
mod tests;
