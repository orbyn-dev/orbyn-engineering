//! Per-hunk provenance recorded at merge: which new-file line ranges the merge brought in, who wrote each (the
//! task's engine and model, or a person who committed on the change) and a local git note that carries the same
//! ids. Parsing git's diff text is pure and separate from running git. Ids, ranges and counts only: never file
//! content, a prompt, a secret or a path outside the repository.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use serde_json::{Value, json};

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
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Author {
    Agent {
        engine: String,
        model: Option<String>,
    },
    Person,
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

/// Splits one merged hunk into contiguous runs of the same author. A line is a person's only if blame names a commit
/// in `person`; a line blame did not name is the agent's.
pub fn split_by_author(
    hunk: &Hunk,
    blame: &BTreeMap<u32, String>,
    person: &HashSet<String>,
    agent: &Author,
) -> Vec<AuthoredHunk> {
    let mut out: Vec<AuthoredHunk> = Vec::new();
    for line in hunk.start..hunk.start + hunk.len {
        let is_person = blame.get(&line).is_some_and(|id| person.contains(id));
        let author = if is_person {
            Author::Person
        } else {
            agent.clone()
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
/// `type` is `agent` or `person`, with the engine and model ids for an agent. Nothing else goes in.
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

/// Gives each hunk its author by `git blame` of the merge commit: a line is a person's only when the commit that last
/// changed it is one of `person`. Anything else is the agent's: an unknown commit, a rewritten id, a conflict
/// resolution, a line blame did not name and a file whose blame failed. One blame runs per file, without `-L`, and
/// the result is sliced, so the number of hunks cannot grow the command line. Repository config cannot reattribute
/// lines: the ignore-revs list is cleared.
pub(crate) async fn attribute_by_blame(
    repo: &Path,
    merge: &str,
    hunks: Vec<Hunk>,
    person: &HashSet<String>,
    agent: &Author,
) -> Vec<AuthoredHunk> {
    if person.is_empty() {
        return hunks
            .into_iter()
            .map(|hunk| AuthoredHunk {
                hunk,
                author: agent.clone(),
            })
            .collect();
    }
    let mut paths: Vec<&str> = hunks.iter().map(|h| h.path.as_str()).collect();
    paths.sort_unstable();
    paths.dedup();
    let mut blamed: BTreeMap<String, BTreeMap<u32, String>> = BTreeMap::new();
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
        }
    }
    let none = BTreeMap::new();
    hunks
        .iter()
        .flat_map(|hunk| {
            split_by_author(hunk, blamed.get(&hunk.path).unwrap_or(&none), person, agent)
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

/// Writes the note on the merge commit, replacing an earlier one. Local only. The body goes through a scratch file
/// (`-F`), not the command line, so its size cannot hit a command-line limit.
async fn write_note(repo: &Path, merge: &str, body: &Value) -> Result<()> {
    need_revision(merge)?;
    let scratch = Scratch::create(repo, &serde_json::to_vec(body)?).await?;
    let file_text = scratch.0.to_string_lossy().into_owned();
    let notes_ref = format!("--ref={NOTE_REF}");
    let mut args: Vec<&str> = NOTE_IDENTITY.to_vec();
    args.extend(["notes", &notes_ref, "add", "-f", "-F", &file_text, merge]);
    git(repo, &args).await?;
    Ok(())
}

/// Records a merge's hunks and its local git note. Hunks come from the diff of `before..merge`.
///
/// `person_commits` are full commit ids positively known to be a person's (for example commits the person made
/// through the app). `git blame` of the merge commit gives each line to the commit that last changed it; a line is
/// a person's only if that commit is in the list. Anything not positively a person's is the agent's: an empty list,
/// an unknown or rewritten id, a conflict resolution, a failed blame. Author emails are never read. The caller must
/// pass only commits it positively identified, never a guess.
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
    let authored = attribute_by_blame(repo, &merge.merge_commit, hunks, &person, agent).await;
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
