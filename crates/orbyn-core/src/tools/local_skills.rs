//! The device-local skills area (KC9): the one place an approved Crew skill proposal
//! writes. Each skill is one folder per slug under `<data>/skills-local`, holding a
//! `SKILL.md` with front matter and a `local_revision` number. The area is
//! outside the shared library on purpose: adding a skill here can never make the
//! library dirty, and nothing is ever written into a target repository or the library.
//!
//! The write is atomic: the complete file is staged beside its destination and renamed
//! into place, so a reader sees the old skill or the new one, never half a file. The
//! revision makes an edit from a stale read fail instead of overwriting a newer text, and
//! an edit carries the name it was read under, so rewriting the front-matter name cannot
//! land on another skill. A create reclaims a folder a crash left empty or holding only
//! the writer's own staging file; a folder that holds anything else is a damaged skill and
//! stays refused. Only slug names are accepted, and every resolved path must stay inside
//! the skill's own folder, so neither `..` nor a symlink can reach anything else. The
//! stored file, its revision line included, is capped
//! at [`MAX_SKILL_BYTES`], and that same cap applies when it is read back.
//!
//! A local skill is personal to this device and applies to every project; the repository a
//! proposal came from is in the proposal's text only and cannot narrow where the skill
//! applies. The shared library always wins a name it holds: a local skill may not be
//! created over one, `skill_load` reads the library one, and `skill_search` lists it once.
//!
//! The one writer is `SkillService::write_local_skill`; the Crew proposals service calls
//! it when a person approves. Engines reach the result through the MCP `skill_search`
//! and `skill_load` tools only: native discovery of a local skill is not built yet.

use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use crate::error::{CoreError, Result};
use crate::redaction::Redactor;
use crate::store::{AuditRecord, Store};
use crate::tools::skills::parse_frontmatter;
use crate::util::{canonicalize, new_id};

/// The most a stored `SKILL.md` may hold, in bytes, its `local_revision` line included.
pub const MAX_SKILL_BYTES: usize = 64 * 1024;
/// The longest skill name, in characters.
pub const MAX_NAME_CHARS: usize = 64;
/// The front-matter key that carries the skill's own revision. The library's revision
/// and dirty flag never include a local skill; this number is how a stale edit is refused.
pub const REVISION_KEY: &str = "local_revision";
/// The area's folder name under the data folder. A worker's confinement refuses any allowance that overlaps it.
pub const AREA_DIR: &str = "skills-local";
const SKILL_FILE: &str = "SKILL.md";
/// The contract's exact sentence for a scan refusal (docs/crew-lessons.md, KC9).
const SECRET_MESSAGE: &str =
    "Not saved: this looks like it holds a secret. Edit it and save again.";
/// The audit action a write records, before the file becomes visible.
pub const AUDIT_ACTION: &str = "skills.local.write";
/// Names Windows cannot use as a folder, refused on every platform so a skill made on
/// macOS or Linux is never unreadable on a Windows machine.
const WINDOWS_RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// A skill that exists in `skills-local`, as the area reads it back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalSkill {
    pub name: String,
    pub description: String,
    /// The skill's own revision from its front matter; 0 for a hand-made file without one.
    pub local_revision: u32,
}

/// Which existing local skill an edit replaces, and at which revision it was read. The
/// name is carried separately from the text so an edit can never land on a different
/// skill by rewriting the front-matter name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalSkillEdit {
    pub name: String,
    pub local_revision: u32,
}

/// What one write stores. The text is the complete `SKILL.md`, front matter included,
/// as the person approved or edited it; the ids are for the audit row only.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalSkillWrite {
    pub text: String,
    /// `None` creates a new local skill and is refused when the name already exists.
    /// `Some(edit)` replaces that named local skill and is refused when it has changed
    /// since that revision was read, or when the text changes its name.
    pub edit: Option<LocalSkillEdit>,
    /// The proposal this write came from, an id only, for the audit row.
    pub proposal: Option<String>,
    /// The proposal's source lessons, ids only, for the audit row.
    pub lessons: Vec<String>,
}

impl LocalSkillWrite {
    pub fn create(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    /// Replaces the local skill `name` read at `local_revision`; the text in `text` must
    /// keep that name.
    pub fn edit(name: impl Into<String>, text: impl Into<String>, local_revision: u32) -> Self {
        Self {
            text: text.into(),
            edit: Some(LocalSkillEdit {
                name: name.into(),
                local_revision,
            }),
            ..Self::default()
        }
    }

    /// Names the proposal and its source lessons for the audit row; never the text.
    pub fn from_proposal(mut self, proposal: impl Into<String>, lessons: Vec<String>) -> Self {
        self.proposal = Some(proposal.into());
        self.lessons = lessons;
        self
    }
}

/// The `<data>/skills-local` area: its files, its revision and its one writer.
pub struct LocalSkills {
    root: PathBuf,
    store: Arc<Store>,
    /// One write at a time, so a create's name check and its folder creation cannot race,
    /// and two edits cannot both pass the revision check.
    writing: Mutex<()>,
    /// Test-only: the next write publishes nothing, so a failed publish's rollback can be
    /// exercised without a real rename failure.
    #[cfg(test)]
    fail_publish: std::sync::atomic::AtomicBool,
}

impl LocalSkills {
    pub fn new(data_dir: &Path, store: Arc<Store>) -> Self {
        Self {
            root: data_dir.join(AREA_DIR),
            store,
            writing: Mutex::new(()),
            #[cfg(test)]
            fail_publish: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Test-only: makes the next write's publish fail after its audit row, as a rename
    /// failure would.
    #[cfg(test)]
    pub(crate) fn fail_next_publish(&self) {
        self.fail_publish
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    #[cfg(test)]
    fn made_publish_fail(&self) -> bool {
        self.fail_publish
            .swap(false, std::sync::atomic::Ordering::SeqCst)
    }

    #[cfg(not(test))]
    fn made_publish_fail(&self) -> bool {
        false
    }

    /// Where the area lives, for tests and for a message that names it.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Every skill the area holds, by name. A missing area is no skills, not an error.
    /// An entry that is not a valid skill of its own name is left out (it could not be
    /// delivered safely), so one damaged folder cannot break every run.
    pub fn list(&self) -> Result<Vec<LocalSkill>> {
        let Some(root) = self.existing_root()? else {
            return Ok(Vec::new());
        };
        let mut skills = Vec::new();
        for entry in std::fs::read_dir(&root)? {
            let entry = entry?;
            // A directory only: a link in the area is not followed by the listing.
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if !valid_name(&name) {
                continue;
            }
            if let Ok(skill) = self.get(&name) {
                skills.push(skill);
            }
        }
        skills.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(skills)
    }

    /// One skill by name; `not_found` when the area has no such skill.
    pub fn get(&self, name: &str) -> Result<LocalSkill> {
        let text = self.read(name, None)?;
        let (from_file, description) = parse_frontmatter(&text)
            .ok_or_else(|| CoreError::Unreadable(format!("skill {name} has no front matter")))?;
        if from_file != name {
            return Err(CoreError::Unreadable(format!(
                "skill {name} names itself {from_file}"
            )));
        }
        Ok(LocalSkill {
            name: name.to_owned(),
            description,
            local_revision: revision_of(&text).unwrap_or(0),
        })
    }

    /// Whether the area holds a readable skill of a valid name, so the MCP read knows to
    /// look here when the shared library does not hold the name.
    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_ok()
    }

    /// Reads `SKILL.md` or a file it references. The name is a slug and the file is a
    /// plain relative path; the resolved file must stay inside that skill's folder.
    pub fn read(&self, name: &str, file: Option<&str>) -> Result<String> {
        let directory = self.skill_dir(name)?;
        let relative = Path::new(file.unwrap_or(SKILL_FILE));
        if !relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        {
            return Err(CoreError::NotFound("skill file".into()));
        }
        let path = canonicalize(&directory.join(relative))
            .map_err(|_| CoreError::NotFound("skill file".into()))?;
        if !path.starts_with(&directory) {
            return Err(CoreError::NotFound("skill file".into()));
        }
        // The skill file itself is capped however it is named, `None` or an explicit
        // spelling; compare the canonical target so a case-insensitive filesystem cannot
        // read `SKILL.md` through `skill.md` uncapped. A referenced file may be larger.
        let skill_file = canonicalize(&directory.join(SKILL_FILE)).ok();
        if skill_file.as_deref() == Some(path.as_path()) {
            let size = std::fs::metadata(&path)?.len();
            if size > MAX_SKILL_BYTES as u64 {
                return Err(too_large());
            }
        }
        Ok(std::fs::read_to_string(path)?)
    }

    /// Writes one skill, after the scan and the revision checks, as one audit row and one
    /// atomic file. `reserved` is the shared library's own names: a local skill may not
    /// shadow one, so `skill_load` always has one answer.
    ///
    /// A create reclaims a folder a crash left empty, or holding only its own staging file,
    /// between making it and publishing; a folder that holds anything else stays refused,
    /// so a damaged skill is never overwritten. An edit must keep the name it was read
    /// under.
    ///
    /// The audit row is appended before the rename, as rule 6 requires: a publish that
    /// fails after it can leave the attempt's row, while the file itself is unchanged. The
    /// file is staged inside the skill's own folder first, so a failure before that rename
    /// leaves the previous skill (or none) exactly as it was.
    pub(crate) fn write(&self, write: LocalSkillWrite, reserved: &[String]) -> Result<LocalSkill> {
        let (name, description) = parse_write(&write.text)?;
        if self.root_is_link() {
            return Err(invalid(
                "The skills-local folder is a link, so nothing is saved there. Replace it with a real folder.",
            ));
        }
        let _writing = self
            .writing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (current, text, revision) = match &write.edit {
            Some(edit) => {
                if name != edit.name {
                    return Err(invalid(
                        "A skill keeps its name when it is edited. Open the skill it belongs to.",
                    ));
                }
                let current = self.get(&edit.name)?;
                if current.local_revision != edit.local_revision {
                    return Err(stale());
                }
                // A revision that cannot advance would let two edits both pass the stale check.
                let Some(revision) = current.local_revision.checked_add(1) else {
                    return Err(stale());
                };
                let text = capped(&write.text, revision)?;
                (Some(current), text, revision)
            }
            None => {
                // The cap is checked before anything on disk can change, so a refused
                // write leaves the area exactly as it was.
                let revision = 1;
                let text = capped(&write.text, revision)?;
                if reserved.iter().any(|reserved| reserved == &name) {
                    return Err(name_taken_library(&name));
                }
                if let Ok(directory) = self.skill_dir(&name) {
                    match self.get(&name) {
                        Ok(_) => return Err(name_taken_local(&name)),
                        // A crash between making the folder and publishing its file
                        // leaves it empty or holding only our staging file; reclaim it.
                        Err(_) if is_reclaimable_orphan(&directory) => {
                            std::fs::remove_dir_all(&directory)?;
                        }
                        Err(_) => return Err(name_taken_local(&name)),
                    }
                }
                (None, text, revision)
            }
        };
        std::fs::create_dir_all(&self.root)?;
        let root = canonicalize(&self.root)?;
        let directory = root.join(&name);
        let created = current.is_none();
        if created {
            match std::fs::create_dir(&directory) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    return Err(name_taken_local(&name));
                }
                Err(error) => return Err(error.into()),
            }
        }
        let staged = match Staging::stage(&directory, text.as_bytes()) {
            Ok(staged) => staged,
            Err(error) => {
                if created {
                    let _ = std::fs::remove_dir(&directory);
                }
                return Err(error);
            }
        };
        let detail = json!({
            "name": name,
            "local_revision": revision,
            "created": created,
            "proposal": write.proposal,
            "lessons": write.lessons,
        });
        if let Err(error) = self.store.append_audit(&AuditRecord {
            actor: "user".into(),
            action: AUDIT_ACTION.into(),
            target: name.clone(),
            decision: None,
            source: "user".into(),
            thread_id: None,
            run_id: None,
            detail,
        }) {
            drop(staged);
            if created {
                let _ = std::fs::remove_dir(&directory);
            }
            return Err(error);
        }
        if self.made_publish_fail() {
            drop(staged);
            if created {
                let _ = std::fs::remove_dir(&directory);
            }
            return Err(CoreError::Internal("the test made the publish fail".into()));
        }
        if let Err(error) = staged.publish(&directory.join(SKILL_FILE)) {
            if created {
                let _ = std::fs::remove_dir(&directory);
            }
            return Err(error);
        }
        Ok(LocalSkill {
            name,
            description,
            local_revision: revision,
        })
    }

    /// The canonical skill folder, which must be a direct child of the canonical area.
    fn skill_dir(&self, name: &str) -> Result<PathBuf> {
        if !valid_name(name) {
            return Err(CoreError::NotFound(format!("skill {name}")));
        }
        let Some(root) = self.existing_root()? else {
            return Err(CoreError::NotFound(format!("skill {name}")));
        };
        let directory = canonicalize(&root.join(name))
            .map_err(|_| CoreError::NotFound(format!("skill {name}")))?;
        if directory.parent() != Some(root.as_path()) || !directory.is_dir() {
            return Err(CoreError::NotFound(format!("skill {name}")));
        }
        Ok(directory)
    }

    /// Whether the area's own path is a symbolic link. A Windows junction is not detected here (it needs a Windows run).
    fn root_is_link(&self) -> bool {
        std::fs::symlink_metadata(&self.root)
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
    }

    fn existing_root(&self) -> Result<Option<PathBuf>> {
        // The area is a real folder inside the data folder: a link there could point anywhere, so it is treated as no area.
        if self.root_is_link() {
            return Ok(None);
        }
        match std::fs::metadata(&self.root) {
            Ok(metadata) if metadata.is_dir() => Ok(Some(canonicalize(&self.root)?)),
            Ok(_) => Ok(None),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}

/// A complete file staged beside its destination. Publishing renames it into place;
/// dropping it without publishing removes only its own staging file.
struct Staging(Option<PathBuf>);

impl Staging {
    fn stage(directory: &Path, bytes: &[u8]) -> Result<Self> {
        let path = directory.join(staging_name(&new_id()));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let staging = Self(Some(path));
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        Ok(staging)
    }

    fn publish(mut self, target: &Path) -> Result<()> {
        let staged = self.0.as_ref().expect("an owned staging file").clone();
        match std::fs::rename(&staged, target) {
            Ok(()) => {
                self.0 = None;
                Ok(())
            }
            Err(error) => Err(error.into()),
        }
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        if let Some(path) = self.0.take()
            && let Err(error) = std::fs::remove_file(&path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%error, "could not remove a staging skill file");
        }
    }
}

/// Whether `name` is a skill slug: 1 to 64 lowercase ASCII letters, numbers and
/// single hyphens, not starting or ending with one, and usable as a folder on Windows.
pub fn valid_name(name: &str) -> bool {
    if name.is_empty()
        || name.len() > MAX_NAME_CHARS
        || name.starts_with('-')
        || name.ends_with('-')
        || name.contains("--")
        || WINDOWS_RESERVED.contains(&name)
    {
        return false;
    }
    name.bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// Parses the front matter a write must have and refuses a secret. The scan reads the
/// same locked formats Crew's scan uses, so a person's Settings cannot narrow it.
fn parse_write(text: &str) -> Result<(String, String)> {
    if text.len() > MAX_SKILL_BYTES {
        return Err(too_large());
    }
    let (name, description) = parse_frontmatter(text).ok_or_else(|| {
        invalid("A skill needs front matter with a name at the top of its SKILL.md.")
    })?;
    if !valid_name(&name) {
        return Err(invalid(
            "A skill name is 1 to 64 lowercase letters, numbers and hyphens.",
        ));
    }
    if Redactor::locked().class_of(text).is_some() {
        return Err(secret_refused());
    }
    Ok((name, description))
}

/// The stored text for `revision`, refused when the file would be over the cap. The cap
/// is on the stored file, its revision line included, so nothing the writer accepts is
/// later refused by its own reader.
fn capped(text: &str, revision: u32) -> Result<String> {
    let text = with_revision(text, revision);
    if text.len() > MAX_SKILL_BYTES {
        return Err(too_large());
    }
    Ok(text)
}

/// The text with its front-matter revision set to `revision`, and nothing else changed.
fn with_revision(text: &str, revision: u32) -> String {
    let Some(rest) = text.strip_prefix("---") else {
        return text.to_owned();
    };
    let Some(end) = rest.find("\n---") else {
        return text.to_owned();
    };
    let mut header: Vec<String> = Vec::new();
    for line in rest[..end].split('\n') {
        let key = line.split_once(':').map(|(key, _)| key.trim());
        if key != Some(REVISION_KEY) {
            header.push(line.to_owned());
        }
    }
    header.push(format!("{REVISION_KEY}: {revision}"));
    format!("---{}{}", header.join("\n"), &rest[end..])
}

fn revision_of(text: &str) -> Option<u32> {
    let rest = text.strip_prefix("---")?;
    let end = rest.find("\n---")?;
    for line in rest[..end].lines() {
        if let Some((key, value)) = line.split_once(':')
            && key.trim() == REVISION_KEY
        {
            return value.trim().parse().ok();
        }
    }
    None
}

fn refused(
    status: u16,
    code: &'static str,
    message: impl Into<String>,
    detail: Value,
) -> CoreError {
    CoreError::Refused {
        status,
        code,
        message: message.into(),
        detail,
    }
}

fn invalid(message: impl Into<String>) -> CoreError {
    refused(400, "invalid", message, json!({}))
}

fn secret_refused() -> CoreError {
    refused(400, "secret_refused", SECRET_MESSAGE, json!({}))
}

fn too_large() -> CoreError {
    refused(
        400,
        "invalid",
        "A skill file holds at most 64 KiB. Shorten it and save again.",
        json!({ "limit": MAX_SKILL_BYTES }),
    )
}

fn name_taken_local(name: &str) -> CoreError {
    refused(
        409,
        "name_taken",
        format!("A skill named {name} already exists on this device. Edit it instead."),
        json!({ "field": "name" }),
    )
}

/// A shared-library skill is not editable here, so a collision asks for another name.
fn name_taken_library(name: &str) -> CoreError {
    refused(
        409,
        "name_taken",
        format!("A skill named {name} already exists in the shared library. Choose another name."),
        json!({ "field": "name" }),
    )
}

/// The name a write stages its file under inside the skill's own folder.
fn staging_name(id: &str) -> String {
    format!(".{SKILL_FILE}.{id}.tmp")
}

/// Whether `name` is one of the writer's staging names, never a person's file.
fn is_staging_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix(&format!(".{SKILL_FILE}.")) else {
        return false;
    };
    rest.strip_suffix(".tmp").is_some_and(|id| !id.is_empty())
}

/// Whether a folder holds only what a crash between making it and publishing its file can
/// leave: nothing, or the writer's own staging file(s). Anything else is a damaged skill
/// and is never overwritten.
fn is_reclaimable_orphan(path: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(path) else {
        return false;
    };
    for entry in entries {
        let Ok(entry) = entry else {
            return false;
        };
        let Ok(name) = entry.file_name().into_string() else {
            return false;
        };
        if !is_staging_name(&name) {
            return false;
        }
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            return false;
        }
    }
    true
}

fn stale() -> CoreError {
    refused(
        409,
        "stale",
        "This skill changed on this device. Review it and save again.",
        json!({}),
    )
}

#[cfg(test)]
mod tests;
