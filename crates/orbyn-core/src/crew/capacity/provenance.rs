//! Per-hunk provenance recorded at merge: which new-file line ranges the merge brought in, who wrote each (the
//! task's engine and model, or a person who committed on the change) and a local git note that carries the same
//! ids. Parsing git's diff text is pure and separate from running git. Ids, ranges and counts only: never file
//! content, a prompt, a secret or a path outside the repository.

use std::path::Path;

use serde_json::{Value, json};

use crate::error::{CoreError, Result};
use crate::source_control::git::{git, git_byte_exact};
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

fn overlaps(a: &Hunk, b: &Hunk) -> bool {
    a.path == b.path && a.start < b.start + b.len && b.start < a.start + a.len
}

/// Gives each merged hunk its author: a person when a person's own commit touched an overlapping range of the same
/// path, the agent otherwise. A person's range comes from that commit's own diff, so a later commit that shifted the lines
/// can leave it a few lines off; the whole merged hunk counts as the person's.
pub fn attribute(hunks: Vec<Hunk>, person: &[Hunk], agent: &Author) -> Vec<AuthoredHunk> {
    hunks
        .into_iter()
        .map(|hunk| {
            let author = if person.iter().any(|p| overlaps(&hunk, p)) {
                Author::Person
            } else {
                agent.clone()
            };
            AuthoredHunk { hunk, author }
        })
        .collect()
}

/// The commits of a `git log --format=%H<TAB>%ae` text whose author email is one of `person_emails` (compared
/// without case).
fn person_commits(log: &str, person_emails: &[String]) -> Vec<String> {
    log.lines()
        .filter_map(|line| line.split_once('\t'))
        .filter(|(_, email)| {
            person_emails
                .iter()
                .any(|known| known.eq_ignore_ascii_case(email.trim()))
        })
        .map(|(hash, _)| hash.to_owned())
        .collect()
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

/// The ranges of the commits in `before..merge` that a person committed (author email in `person_emails`).
async fn person_hunks(
    repo: &Path,
    before: &str,
    merge: &str,
    person_emails: &[String],
) -> Result<Vec<Hunk>> {
    if person_emails.is_empty() {
        return Ok(Vec::new());
    }
    let range = format!("{before}..{merge}");
    let log = git_byte_exact(
        repo,
        &["log", "--no-merges", "--format=%H%x09%ae", &range, "--"],
    )
    .await?;
    let mut ranges = Vec::new();
    for commit in person_commits(&String::from_utf8_lossy(&log), person_emails) {
        need_revision(&commit)?;
        let mut args = vec!["show", "--format="];
        args.extend(DIFF_ARGS);
        args.extend([commit.as_str(), "--"]);
        ranges.extend(parse_diff(&git_byte_exact(repo, &args).await?));
    }
    Ok(ranges)
}

/// Writes the note on the merge commit, replacing an earlier one. Local only.
async fn write_note(repo: &Path, merge: &str, body: &Value) -> Result<()> {
    need_revision(merge)?;
    let text = serde_json::to_string(body)?;
    let mut args: Vec<&str> = NOTE_IDENTITY.to_vec();
    let notes_ref = format!("--ref={NOTE_REF}");
    args.extend(["notes", &notes_ref, "add", "-f", "-m", &text, merge]);
    git(repo, &args).await?;
    Ok(())
}

/// Records a merge's hunks and its local git note. Hunks are found from the diff of `before..merge`; `agent` wrote them
/// except where a commit by one of `person_emails` touched the same lines. The store write is one transaction and its
/// error is returned. The note is written after it, and a failure there is returned inside `Recorded` for the caller
/// to log, because the merge has already happened.
pub async fn record_at_merge(
    store: &Store,
    merge: &MergeRecord,
    agent: &Author,
    person_emails: &[String],
    now: i64,
) -> Result<Recorded> {
    let repo = Path::new(&merge.repo);
    let hunks = hunks_of(repo, &merge.before_commit, &merge.merge_commit).await?;
    let people = person_hunks(
        repo,
        &merge.before_commit,
        &merge.merge_commit,
        person_emails,
    )
    .await?;
    let authored = attribute(hunks, &people, agent);
    store.record_crew_hunks(merge.id, &merge.repo, now, &authored)?;
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
