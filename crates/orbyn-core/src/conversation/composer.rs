//! What a message needs before it can be sent: attachments and their limits,
//! mention lookups and the slash command list. Attachments live in app data,
//! never in a target.

use std::path::{Path, PathBuf};

use orbyn_contracts::{
    Attachment, AttachmentSource, EngineTier, Mentions, SlashCommand, ThreadMention,
};

use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::source_control::git::git;
use crate::tools::claude_skills::{self, ClaudeSkill};
use crate::util::{new_id, now};

pub const MAX_PROMPT_CHARS: usize = 120_000;
pub const MAX_ATTACHMENTS: usize = 100;
pub const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;
pub const MAX_IMAGES_TOTAL: u64 = 80 * 1024 * 1024;
pub const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;
pub const MAX_SOURCE_APP_NAME_CHARS: usize = 255;
pub const MAX_SOURCE_WINDOW_TITLE_CHARS: usize = 1_000;
pub const MAX_SOURCE_APP_ID_CHARS: usize = 255;
pub const MAX_SOURCE_ACCESSIBLE_TEXT_CHARS: usize = 32_000;
pub const MAX_SOURCE_CAPTURED_AT_CHARS: usize = 64;
const MAX_FILE_RESULTS: usize = 50;
const MAX_THREAD_RESULTS: usize = 20;
const MAX_WALKED_FILES: usize = 50_000;
const MAX_COMMAND_FILES: usize = 500;

pub fn check_prompt(prompt: &str) -> Result<()> {
    if prompt.trim().is_empty() {
        return Err(CoreError::Invalid("Enter a prompt to start a run.".into()));
    }
    if prompt.chars().count() > MAX_PROMPT_CHARS {
        return Err(CoreError::Invalid(format!(
            "Messages can be up to {MAX_PROMPT_CHARS} characters."
        )));
    }
    Ok(())
}

fn is_image(mime: &str) -> bool {
    mime.starts_with("image/")
}

fn unsupported_source_type() -> CoreError {
    CoreError::Refused {
        status: 422,
        code: "unsupported_type",
        message: "Only an image can carry a Snap Shot source.".into(),
        detail: serde_json::Value::Null,
    }
}

/// Checks an attachment source against its limits, counted in characters.
fn check_source(source: &AttachmentSource) -> Result<()> {
    let AttachmentSource::SnapShot {
        captured_at,
        app_name,
        window_title,
        app_id,
        accessible_text,
    } = source;
    if app_name.is_empty() {
        return Err(CoreError::Invalid(
            "Give the Snap Shot its app's name.".into(),
        ));
    }
    if captured_at.is_empty() {
        return Err(CoreError::Invalid(
            "The Snap Shot's captured time is missing.".into(),
        ));
    }
    let fields: [(&'static str, &str, usize); 5] = [
        ("app_name", app_name.as_str(), MAX_SOURCE_APP_NAME_CHARS),
        (
            "window_title",
            window_title.as_str(),
            MAX_SOURCE_WINDOW_TITLE_CHARS,
        ),
        (
            "app_id",
            app_id.as_deref().unwrap_or_default(),
            MAX_SOURCE_APP_ID_CHARS,
        ),
        (
            "accessible_text",
            accessible_text.as_deref().unwrap_or_default(),
            MAX_SOURCE_ACCESSIBLE_TEXT_CHARS,
        ),
        (
            "captured_at",
            captured_at.as_str(),
            MAX_SOURCE_CAPTURED_AT_CHARS,
        ),
    ];
    for (field, text, limit) in fields {
        if text.chars().count() > limit {
            return Err(CoreError::SourceTooLarge { field, limit });
        }
    }
    Ok(())
}

/// The file name alone, so a name cannot steer where the upload lands.
pub(crate) fn safe_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = base
        .chars()
        .map(|c| if c.is_control() { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_start_matches('.');
    if trimmed.is_empty() {
        "attachment".to_owned()
    } else {
        trimmed.chars().take(200).collect()
    }
}

fn check_limits(existing: &[Attachment], mime: &str, size: u64) -> Result<()> {
    if existing.len() >= MAX_ATTACHMENTS {
        return Err(CoreError::Invalid(
            "A thread can hold 100 attachments.".into(),
        ));
    }
    if is_image(mime) {
        if size > MAX_IMAGE_BYTES {
            return Err(CoreError::Invalid("Images can be up to 10 MiB.".into()));
        }
        let total: u64 = existing
            .iter()
            .filter(|attachment| is_image(&attachment.mime))
            .map(|attachment| attachment.size)
            .sum();
        if total + size > MAX_IMAGES_TOTAL {
            return Err(CoreError::Invalid("Images can total up to 80 MiB.".into()));
        }
    } else if size > MAX_FILE_BYTES {
        return Err(CoreError::Invalid("Files can be up to 50 MiB.".into()));
    }
    Ok(())
}

impl Core {
    fn attachments_dir(&self) -> PathBuf {
        self.config.data_dir.join("attachments")
    }

    /// Stores an upload. The record exists only once the bytes are in place, so a
    /// message can never reference a half-written file.
    pub fn add_attachment(
        &self,
        thread_id: &str,
        name: &str,
        mime: &str,
        bytes: &[u8],
    ) -> Result<Attachment> {
        self.store.thread(thread_id)?;
        let existing: Vec<Attachment> = self
            .store
            .list_attachments(thread_id)?
            .into_iter()
            .map(|(attachment, _)| attachment)
            .collect();
        check_limits(&existing, mime, bytes.len() as u64)?;
        let id = new_id();
        let name = safe_name(name);
        let dir = self.attachments_dir();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{id}-{name}"));
        let partial = dir.join(format!("{id}.partial"));
        std::fs::write(&partial, bytes)?;
        std::fs::rename(&partial, &path)?;
        let attachment = Attachment {
            id,
            thread_id: thread_id.to_owned(),
            name,
            mime: mime.to_owned(),
            size: bytes.len() as u64,
            created_at: now(),
            source: None,
        };
        if let Err(error) = self
            .store
            .add_attachment(&attachment, &path.to_string_lossy())
        {
            let _ = std::fs::remove_file(&path);
            return Err(error);
        }
        Ok(attachment)
    }

    pub fn remove_attachment(&self, id: &str) -> Result<()> {
        let path = self.store.remove_attachment(id)?;
        let _ = std::fs::remove_file(path);
        Ok(())
    }

    /// Keeps what the page knows about an image's origin, before it is sent. An attachment a
    /// queued message or a run already names is no longer a draft, so it answers like an
    /// unknown one.
    pub fn set_attachment_source(
        &self,
        attachment_id: &str,
        source: AttachmentSource,
    ) -> Result<Attachment> {
        let (attachment, _) = self.store.attachment(attachment_id)?;
        if self
            .store
            .attachment_is_sent(attachment_id, &attachment.thread_id)?
        {
            return Err(CoreError::NotFound("attachment".into()));
        }
        if !is_image(&attachment.mime) {
            return Err(unsupported_source_type());
        }
        check_source(&source)?;
        self.store.set_attachment_source(attachment_id, &source)?;
        Ok(self.store.attachment(attachment_id)?.0)
    }

    pub async fn mentions(&self, thread_id: &str, query: &str) -> Result<Mentions> {
        let thread = self.store.thread(thread_id)?;
        let needle = query.to_lowercase();
        let driver = self.engines.driver(&thread.engine)?;
        let route = self.skills.route(
            &driver.descriptor().capabilities.skill_delivery,
            thread.skill_profile.as_deref(),
        );
        // A library still being set up has no skills, but files still resolve.
        let mut skills: Vec<String> = route
            .map(|route| route.selection)
            .unwrap_or_default()
            .into_iter()
            .filter(|name| name.to_lowercase().contains(&needle))
            .collect();
        if driver.family() == "claude" {
            for skill in self.claude_skills(Path::new(&thread.cwd)) {
                if skill.name.to_lowercase().contains(&needle) && !skills.contains(&skill.name) {
                    skills.push(skill.name);
                }
            }
        }
        let threads = self
            .store
            .list_threads(Some(&thread.project_id), false)?
            .into_iter()
            .filter(|other| other.id != thread.id)
            .filter(|other| other.title.to_lowercase().contains(&needle))
            .take(MAX_THREAD_RESULTS)
            .map(|other| ThreadMention {
                id: other.id,
                title: other.title,
            })
            .collect();
        Ok(Mentions {
            skills,
            files: matching_files(Path::new(&thread.cwd), &needle).await,
            threads,
        })
    }

    /// What `/` can offer here: the commands Orbyn handles, the commands the
    /// engine reported at its last start, and custom command files, which are
    /// only listed and never run by Orbyn. `with_skills` adds the library skills.
    pub async fn slash_commands(
        &self,
        thread_id: &str,
        with_skills: bool,
    ) -> Result<Vec<SlashCommand>> {
        let thread = self.store.thread(thread_id)?;
        // Asking for the list also fills in commands and agents for an engine that
        // has not had a session yet. A failure just leaves what is stored.
        self.engines.models(&thread.engine, false).await?;
        let driver = self.engines.driver(&thread.engine)?;
        let descriptor = driver.descriptor();
        let native = descriptor.tier == EngineTier::Native;
        let mut commands = Vec::new();
        let mut push = |name: &str, description: &str, source: &str| {
            if !commands.iter().any(|c: &SlashCommand| c.name == name) {
                commands.push(SlashCommand {
                    name: name.to_owned(),
                    description: description.to_owned(),
                    source: source.to_owned(),
                });
            }
        };
        if native {
            push(
                "compact",
                "Summarize the conversation to free up context",
                "orbyn",
            );
            push(
                "goal",
                "Keep working across turns until this is true",
                "orbyn",
            );
        }
        push("plan", "Plan without changing anything", "orbyn");
        push("default", "Leave plan mode", "orbyn");
        push("usage-limits", "Show what is left of your limits", "orbyn");
        for (name, description) in self.store.engine_commands(&thread.engine)? {
            push(&name, &description, "engine");
        }
        for agent in self.store.engine_agents(&thread.engine)? {
            push(&agent.name, &agent.description, "agent");
        }
        for folder in command_folders(&driver.family(), Path::new(&thread.cwd)) {
            for (name, description) in command_files(&folder) {
                push(&name, &description, "engine");
            }
        }
        if with_skills {
            let route = self.skills.route(
                &descriptor.capabilities.skill_delivery,
                thread.skill_profile.as_deref(),
            );
            for name in route.map(|route| route.selection).unwrap_or_default() {
                push(&name, "Library skill", "skill");
            }
            if driver.family() == "claude" {
                for skill in self.claude_skills(Path::new(&thread.cwd)) {
                    push(&skill.name, &skill.description, "skill");
                }
            }
        }
        Ok(commands)
    }

    /// The Claude CLI's own skills for this thread: its config directory, the start
    /// folder (the checkout) and the library it adds. Discovery reads only; the CLI
    /// loads them.
    fn claude_skills(&self, checkout: &Path) -> Vec<ClaudeSkill> {
        let Ok(base) = self.workspace.root() else {
            return Vec::new();
        };
        let Some(config) = claude_skills::config_dir(&base) else {
            return Vec::new();
        };
        let plugin_cache = std::env::var_os(claude_skills::PLUGIN_CACHE_ENV).map(PathBuf::from);
        claude_skills::discover(&config, checkout, &base, plugin_cache.as_deref())
    }
}

/// Where this engine keeps custom commands. Target files are only read.
fn command_folders(engine: &str, checkout: &Path) -> Vec<PathBuf> {
    let home = || {
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
    };
    let dir = |name: &str, fallback: &str| {
        std::env::var_os(name)
            .map(PathBuf::from)
            .or_else(|| home().map(|home| home.join(fallback)))
    };
    match engine {
        "claude" => [
            Some(checkout.join(".claude/commands")),
            dir("CLAUDE_CONFIG_DIR", ".claude").map(|base| base.join("commands")),
        ]
        .into_iter()
        .flatten()
        .collect(),
        "codex" => dir("CODEX_HOME", ".codex")
            .map(|base| base.join("prompts"))
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

/// `name.md` files with the `description:` line of their front matter, sorted.
fn command_files(folder: &Path) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<(String, String)> = entries
        .flatten()
        .take(MAX_COMMAND_FILES)
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                return None;
            }
            let name = path.file_stem()?.to_str()?.to_owned();
            Some((name, front_matter_description(&path).unwrap_or_default()))
        })
        .collect();
    found.sort();
    found
}

fn front_matter_description(path: &Path) -> Option<String> {
    use std::io::{BufRead, BufReader};
    let file = std::fs::File::open(path).ok()?;
    let mut lines = BufReader::new(file)
        .lines()
        .take(20)
        .map_while(|line| line.ok());
    if lines.next()?.trim() != "---" {
        return None;
    }
    lines
        .take_while(|line| line.trim() != "---")
        .find_map(|line| {
            line.strip_prefix("description:")
                .map(|text| text.trim().trim_matches(['"', '\'']).to_owned())
        })
}

/// Lower is better: a name match beats a path match, a prefix beats a substring,
/// and a shorter path breaks the tie.
fn match_rank(path: &str, needle: &str) -> Option<(u8, usize)> {
    let lower = path.to_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    let tier = if needle.is_empty() {
        3
    } else if name == needle {
        0
    } else if name.starts_with(needle) {
        1
    } else if name.contains(needle) {
        2
    } else if lower.contains(needle) {
        3
    } else {
        return None;
    };
    Some((tier, path.len()))
}

fn rank_files(paths: impl Iterator<Item = String>, needle: &str) -> Vec<String> {
    let mut ranked: Vec<((u8, usize), String)> = paths
        .filter_map(|path| Some((match_rank(&path, needle)?, path)))
        .collect();
    ranked.sort();
    ranked
        .into_iter()
        .take(MAX_FILE_RESULTS)
        .map(|(_, path)| path)
        .collect()
}

/// A bounded walk for a folder Git does not track.
fn walk_files(root: &Path) -> Vec<String> {
    const SKIPPED: &[&str] = &[".git", "node_modules", "target", "dist", "build"];
    let mut found = Vec::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            if found.len() >= MAX_WALKED_FILES {
                return found;
            }
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                if !SKIPPED.contains(&name) {
                    folders.push(entry.path());
                }
            } else if kind.is_file()
                && let Ok(relative) = entry.path().strip_prefix(root)
            {
                found.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found
}

/// Tracked and untracked paths, ignoring what the repository ignores.
async fn matching_files(checkout: &Path, needle: &str) -> Vec<String> {
    let listing = git(
        checkout,
        &[
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ],
    )
    .await;
    match listing {
        Ok(listing) => rank_files(
            listing
                .split(|byte| *byte == 0)
                .filter(|path| !path.is_empty())
                .map(|path| String::from_utf8_lossy(path).into_owned()),
            needle,
        ),
        Err(_) => {
            let root = checkout.to_path_buf();
            let walked = tokio::task::spawn_blocking(move || walk_files(&root))
                .await
                .unwrap_or_default();
            rank_files(walked.into_iter(), needle)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;
    use orbyn_contracts::{AgUiEvent, AttachmentSource, ModelSelection};

    fn stored(mime: &str, size: u64) -> Attachment {
        Attachment {
            id: "a".into(),
            thread_id: "t".into(),
            name: "n".into(),
            mime: mime.into(),
            size,
            created_at: String::new(),
            source: None,
        }
    }

    #[test]
    fn file_matches_rank_names_above_paths() {
        let paths = [
            "src/app/main.rs",
            "main.rs",
            "docs/main-notes.md",
            "maintainers/a",
        ];
        let ranked = rank_files(paths.iter().map(|path| (*path).to_owned()), "main.rs");
        assert_eq!(ranked, ["main.rs", "src/app/main.rs"]);
        let ranked = rank_files(paths.iter().map(|path| (*path).to_owned()), "main");
        assert_eq!(ranked[0], "main.rs");
        assert_eq!(ranked.last().unwrap(), "maintainers/a");
    }

    #[test]
    fn command_files_keep_their_front_matter_description() {
        let dir = std::env::temp_dir().join(new_id());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("ship.md"),
            "---\ndescription: \"Ship it\"\n---\nbody",
        )
        .unwrap();
        std::fs::write(dir.join("plain.md"), "no front matter").unwrap();
        std::fs::write(dir.join("notes.txt"), "not a command").unwrap();
        assert_eq!(
            command_files(&dir),
            [
                ("plain".to_owned(), String::new()),
                ("ship".to_owned(), "Ship it".to_owned())
            ]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn prompt_limit_counts_characters_not_bytes() {
        assert!(check_prompt(&"é".repeat(MAX_PROMPT_CHARS)).is_ok());
        assert!(check_prompt(&"x".repeat(MAX_PROMPT_CHARS + 1)).is_err());
        assert!(check_prompt("   ").is_err());
    }

    #[test]
    fn limits_follow_kind_and_totals() {
        assert!(check_limits(&[], "image/png", MAX_IMAGE_BYTES).is_ok());
        assert!(check_limits(&[], "image/png", MAX_IMAGE_BYTES + 1).is_err());
        assert!(check_limits(&[], "text/plain", MAX_FILE_BYTES).is_ok());
        assert!(check_limits(&[], "text/plain", MAX_FILE_BYTES + 1).is_err());
        let images = vec![stored("image/png", MAX_IMAGE_BYTES); 8];
        assert!(check_limits(&images, "image/png", 1).is_err());
        assert!(check_limits(&images, "text/plain", 1).is_ok());
        let full = vec![stored("text/plain", 1); MAX_ATTACHMENTS];
        assert!(check_limits(&full, "text/plain", 1).is_err());
    }

    #[test]
    fn names_cannot_steer_the_path() {
        assert_eq!(safe_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_name(r"C:\x\report.pdf"), "report.pdf");
        assert_eq!(safe_name(".."), "attachment");
        assert_eq!(safe_name(""), "attachment");
        assert_eq!(safe_name(".hidden"), "hidden");
    }

    struct SourceFixture {
        _dir: TempDir,
        core: Core,
        thread: String,
    }

    fn source_fixture() -> SourceFixture {
        let dir = TempDir::new();
        let mut config =
            crate::app::CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
        config.engines.lookup_latest = false;
        let core = Core::open(config).unwrap();
        let project = core
            .store
            .add_project(crate::store::NewProject {
                path: "/repo",
                name: "repo",
                is_git: true,
                managed: false,
                scratch: false,
            })
            .unwrap();
        core.store.trust_project(&project.id, "t").unwrap();
        let thread = core
            .store
            .create_thread(crate::store::NewThread {
                project_id: project.id,
                title: "t".into(),
                engine: "script".into(),
                model: None,
                cwd: "/repo".into(),
                worktree: None,
                skill_profile: None,
                parent_thread_id: None,
                relation: None,
                scratch: false,
            })
            .unwrap();
        SourceFixture {
            _dir: dir,
            core,
            thread: thread.id,
        }
    }

    impl SourceFixture {
        fn image(&self) -> Attachment {
            self.core
                .add_attachment(&self.thread, "shot.png", "image/png", b"png")
                .unwrap()
        }
    }

    fn snap_shot(app_name: &str, title: &str, text: Option<&str>) -> AttachmentSource {
        AttachmentSource::SnapShot {
            captured_at: "2026-11-22T10:00:00Z".into(),
            app_name: app_name.into(),
            window_title: title.into(),
            app_id: Some("com.example.app".into()),
            accessible_text: text.map(str::to_owned),
        }
    }

    #[test]
    fn a_source_is_saved_and_replaces_the_one_before_it() {
        let f = source_fixture();
        let image = f.image();
        let first = snap_shot("Notes", "Weekly plan", Some("buy milk"));
        let saved = f
            .core
            .set_attachment_source(&image.id, first.clone())
            .unwrap();
        assert_eq!(saved.source, Some(first));

        let replacement = snap_shot("Browser", "Docs", None);
        let saved = f
            .core
            .set_attachment_source(&image.id, replacement.clone())
            .unwrap();
        assert_eq!(saved.source, Some(replacement));
    }

    #[test]
    fn an_unknown_attachment_is_not_found() {
        let f = source_fixture();
        assert!(matches!(
            f.core
                .set_attachment_source("missing", snap_shot("Notes", "Plan", None)),
            Err(CoreError::NotFound(what)) if what == "attachment"
        ));
    }

    #[test]
    fn only_an_image_takes_a_source() {
        let f = source_fixture();
        let file = f
            .core
            .add_attachment(&f.thread, "notes.txt", "text/plain", b"hi")
            .unwrap();
        assert!(matches!(
            f.core
                .set_attachment_source(&file.id, snap_shot("Notes", "Plan", None)),
            Err(CoreError::Refused {
                status: 422,
                code: "unsupported_type",
                ..
            })
        ));
    }

    #[test]
    fn an_attachment_a_queued_message_names_is_already_sent() {
        let f = source_fixture();
        let image = f.image();
        f.core
            .store
            .enqueue(
                &f.thread,
                "later",
                &[image.id.clone()],
                &ModelSelection::default(),
            )
            .unwrap();
        assert!(matches!(
            f.core
                .set_attachment_source(&image.id, snap_shot("Notes", "Plan", None)),
            Err(CoreError::NotFound(what)) if what == "attachment"
        ));
    }

    #[test]
    fn an_attachment_a_sent_run_names_is_already_sent() {
        let f = source_fixture();
        let image = f.image();
        f.core
            .store
            .append_thread_event(
                &f.thread,
                &AgUiEvent::Custom {
                    name: "user.attachments".into(),
                    value: serde_json::json!({ "attachments": [{ "id": image.id.clone() }] }),
                },
            )
            .unwrap();
        assert!(matches!(
            f.core
                .set_attachment_source(&image.id, snap_shot("Notes", "Plan", None)),
            Err(CoreError::NotFound(what)) if what == "attachment"
        ));
    }

    #[test]
    fn the_source_limits_count_characters() {
        let f = source_fixture();
        let image = f.image();
        assert!(
            f.core
                .set_attachment_source(
                    &image.id,
                    AttachmentSource::SnapShot {
                        captured_at: "x".repeat(64),
                        app_name: "é".repeat(255),
                        window_title: "t".repeat(1000),
                        app_id: Some("a".repeat(255)),
                        accessible_text: Some("x".repeat(32_000)),
                    },
                )
                .is_ok()
        );
        for (source, field, limit) in [
            (
                AttachmentSource::SnapShot {
                    captured_at: "2026-11-22T10:00:00Z".into(),
                    app_name: "a".repeat(256),
                    window_title: String::new(),
                    app_id: None,
                    accessible_text: None,
                },
                "app_name",
                255,
            ),
            (
                AttachmentSource::SnapShot {
                    captured_at: "2026-11-22T10:00:00Z".into(),
                    app_name: "Notes".into(),
                    window_title: "a".repeat(1001),
                    app_id: None,
                    accessible_text: None,
                },
                "window_title",
                1000,
            ),
            (
                AttachmentSource::SnapShot {
                    captured_at: "2026-11-22T10:00:00Z".into(),
                    app_name: "Notes".into(),
                    window_title: String::new(),
                    app_id: Some("a".repeat(256)),
                    accessible_text: None,
                },
                "app_id",
                255,
            ),
            (
                AttachmentSource::SnapShot {
                    captured_at: "2026-11-22T10:00:00Z".into(),
                    app_name: "Notes".into(),
                    window_title: String::new(),
                    app_id: None,
                    accessible_text: Some("a".repeat(32_001)),
                },
                "accessible_text",
                32_000,
            ),
            (
                AttachmentSource::SnapShot {
                    captured_at: "a".repeat(65),
                    app_name: "Notes".into(),
                    window_title: String::new(),
                    app_id: None,
                    accessible_text: None,
                },
                "captured_at",
                64,
            ),
        ] {
            assert!(matches!(
                f.core.set_attachment_source(&image.id, source),
                Err(CoreError::SourceTooLarge {
                    field: found,
                    limit: max,
                }) if found == field && max == limit
            ));
        }
    }

    #[test]
    fn an_empty_app_name_or_capture_time_is_invalid() {
        let f = source_fixture();
        let image = f.image();
        for source in [
            snap_shot("", "Plan", None),
            AttachmentSource::SnapShot {
                captured_at: String::new(),
                app_name: "Notes".into(),
                window_title: "Plan".into(),
                app_id: None,
                accessible_text: None,
            },
        ] {
            assert!(matches!(
                f.core.set_attachment_source(&image.id, source),
                Err(CoreError::Invalid(_))
            ));
        }
    }
}
