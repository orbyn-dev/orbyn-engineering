//! Skills are read in place from the workspace library. Engines with native
//! discovery find them on their own; this service backs the MCP tools, the
//! bootstrap list and the settings view. A skill's identity is the library's
//! revision plus its dirty flag.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use orbyn_contracts::{SkillDelivery, SkillPackStatus, SkillProfile, SkillSummary};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::{CoreError, Result};
use crate::project::workspace::{Workspace, native_skills_available};
use crate::store::Store;
use crate::tools::local_skills::{LocalSkill, LocalSkillWrite, LocalSkills};
use crate::util::canonicalize;

pub const DEFAULT_PROFILE: &str = "default";

#[derive(Clone, Debug, PartialEq)]
pub struct LibrarySkill {
    pub name: String,
    pub category: String,
    pub description: String,
}

/// How this run's engine gets its skills.
#[derive(Clone, Debug, PartialEq)]
pub struct SkillRoute {
    /// False sends the engine to the MCP tools.
    pub native: bool,
    pub selection: Vec<String>,
    /// A chosen profile rather than the default set.
    pub limited: bool,
}

pub struct SkillService {
    store: Arc<Store>,
    workspace: Arc<Workspace>,
    /// The device-local skills an approved Crew proposal wrote (local_skills), outside
    /// the library so they never change its revision or dirty flag.
    local: LocalSkills,
}

#[derive(Deserialize)]
struct Catalog {
    skills: Vec<CatalogSkill>,
}

#[derive(Deserialize)]
struct CatalogSkill {
    name: String,
    category: String,
}

/// Reads `name` and `description` from the leading `---` block of a SKILL.md.
pub fn parse_frontmatter(text: &str) -> Option<(String, String)> {
    let body = text.strip_prefix("---")?;
    let end = body.find("\n---")?;
    let mut name = None;
    let mut description = None;
    for line in body[..end].lines() {
        if let Some((key, value)) = line.split_once(':') {
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|rest| rest.strip_suffix('"'))
                .map_or_else(|| value.to_owned(), |inner| inner.replace("\\\"", "\""));
            match key.trim() {
                "name" => name = Some(value),
                "description" => description = Some(value),
                _ => {}
            }
        }
    }
    Some((name?, description.unwrap_or_default()))
}

/// REA and social skills are specialist choices.
pub fn default_selection(skills: &[LibrarySkill]) -> BTreeSet<String> {
    skills
        .iter()
        .filter(|skill| {
            let category = skill.category.to_lowercase();
            !(category.contains("rea") && category.contains("maintenance"))
                && !category.contains("social")
                && !skill.name.starts_with("rea-")
                && !skill.name.starts_with("social-")
        })
        .map(|skill| skill.name.clone())
        .collect()
}

/// Native discovery needs the engine to support it and the links to exist.
pub fn route_is_native(supported: &[SkillDelivery], links_ok: bool) -> bool {
    links_ok && supported.contains(&SkillDelivery::NativeSkillRoot)
}

impl SkillService {
    pub fn new(store: Arc<Store>, workspace: Arc<Workspace>, data_dir: PathBuf) -> Self {
        let local = LocalSkills::new(&data_dir, store.clone());
        Self {
            store,
            workspace,
            local,
        }
    }

    pub fn catalog(&self) -> Result<Vec<LibrarySkill>> {
        let root = match self.workspace.root() {
            Ok(root) => root,
            Err(CoreError::WorkspaceRequired) => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let skills = root.join("skills");
        let catalog: Catalog =
            serde_json::from_slice(&std::fs::read(skills.join("catalog.json"))?)?;
        Ok(catalog
            .skills
            .into_iter()
            .filter_map(|entry| {
                let text =
                    std::fs::read_to_string(skills.join(&entry.name).join("SKILL.md")).ok()?;
                let (name, description) = parse_frontmatter(&text)?;
                (name == entry.name).then_some(LibrarySkill {
                    name,
                    category: entry.category,
                    description,
                })
            })
            .collect())
    }

    pub async fn status(&self) -> Result<SkillPackStatus> {
        let Ok(root) = self.workspace.root() else {
            return Ok(SkillPackStatus {
                version: String::new(),
                digest: String::new(),
                signature_valid: false,
                source: None,
                skills: Vec::new(),
            });
        };
        let (revision, dirty) = self.workspace.identity().await?;
        let skills = self.catalog()?;
        let enabled = default_selection(&skills);
        let short: String = revision.chars().take(12).collect();
        Ok(SkillPackStatus {
            version: if dirty {
                format!("{short}-dirty")
            } else {
                short
            },
            digest: revision,
            signature_valid: true,
            source: Some(root.to_string_lossy().into_owned()),
            skills: skills
                .into_iter()
                .map(|skill| SkillSummary {
                    enabled: enabled.contains(&skill.name),
                    name: skill.name,
                    category: skill.category,
                    description: skill.description,
                })
                .collect(),
        })
    }

    pub fn profiles(&self) -> Result<Vec<SkillProfile>> {
        let mut profiles = vec![SkillProfile {
            id: DEFAULT_PROFILE.into(),
            name: "Default".into(),
            skills: default_selection(&self.catalog()?).into_iter().collect(),
        }];
        profiles.extend(self.store.list_profiles()?);
        Ok(profiles)
    }

    fn selection(&self, profile: Option<&str>) -> Result<Vec<String>> {
        match profile {
            None | Some(DEFAULT_PROFILE) => {
                Ok(default_selection(&self.catalog()?).into_iter().collect())
            }
            Some(id) => self
                .store
                .profile(id)?
                .map(|profile| profile.skills)
                .ok_or_else(|| CoreError::NotFound("skill profile".into())),
        }
    }

    pub fn route(&self, supported: &[SkillDelivery], profile: Option<&str>) -> Result<SkillRoute> {
        let root = self.workspace.root()?;
        Ok(SkillRoute {
            native: route_is_native(supported, native_skills_available(&root)),
            selection: self.selection(profile)?,
            limited: profile.is_some_and(|id| id != DEFAULT_PROFILE),
        })
    }

    /// The skill names a run's MCP `skill_search` and `skill_load` tools may serve: the
    /// route's own selection, plus the device-local skills an approved Crew proposal wrote
    /// while the thread uses the default profile. A chosen profile stays what it names, so
    /// a local skill is never delivered where the person limited the thread. A damaged or
    /// unreadable local area never fails the run: it is logged and the route's own
    /// selection is served.
    pub fn tool_selection(&self, route: &SkillRoute) -> Result<Vec<String>> {
        let mut names = route.selection.clone();
        if route.limited {
            return Ok(names);
        }
        // A name the library holds is the library's: when the default selection leaves it out (a specialist
        // category), a local skill of that name must not switch it back on, because `skill_load` serves the library copy.
        let library: BTreeSet<String> = self
            .catalog()?
            .into_iter()
            .map(|skill| skill.name)
            .collect();
        match self.local.list() {
            Ok(skills) => {
                let local: Vec<String> = skills
                    .into_iter()
                    .map(|skill| skill.name)
                    .filter(|name| !names.contains(name) && !library.contains(name))
                    .collect();
                names.extend(local);
            }
            Err(error) => {
                tracing::warn!(%error, "the skills-local area could not be read; serving the library selection");
            }
        }
        Ok(names)
    }

    /// Writes a skill an approved Crew proposal stores in `skills-local`. This is the
    /// area's one writer; the Crew proposals service calls it when a person approves and
    /// maps its `Refused` errors to its own answer. A name the shared library holds is
    /// reserved, so a local skill never shadows one in `skill_load`.
    pub fn write_local_skill(&self, write: LocalSkillWrite) -> Result<LocalSkill> {
        let reserved: Vec<String> = self
            .catalog()?
            .into_iter()
            .map(|skill| skill.name)
            .collect();
        self.local.write(write, &reserved)
    }

    pub fn search(&self, query: &str, allowed: &[String]) -> Result<Vec<Value>> {
        let terms: Vec<String> = query
            .to_lowercase()
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        let catalog = self.catalog()?;
        // The shared library wins a name it holds: a local skill of the same name is not
        // listed a second time and is never the one an engine reads. A damaged local area
        // never fails the search: it is logged and the library results are returned.
        let local: Vec<LocalSkill> = match self.local.list() {
            Ok(skills) => skills
                .into_iter()
                .filter(|skill| allowed.contains(&skill.name))
                .filter(|skill| !catalog.iter().any(|library| library.name == skill.name))
                .collect(),
            Err(error) => {
                tracing::warn!(%error, "the skills-local area could not be read; searching the library only");
                Vec::new()
            }
        };
        let mut found: Vec<Value> = catalog
            .iter()
            .filter(|skill| allowed.contains(&skill.name))
            .filter(|skill| {
                let text = format!("{} {}", skill.name, skill.description).to_lowercase();
                terms.iter().all(|term| text.contains(term))
            })
            .map(|skill| json!({ "name": skill.name, "description": skill.description }))
            .collect();
        found.extend(
            local
                .into_iter()
                .filter(|skill| {
                    let text = format!("{} {}", skill.name, skill.description).to_lowercase();
                    terms.iter().all(|term| text.contains(term))
                })
                .map(|skill| json!({ "name": skill.name, "description": skill.description })),
        );
        Ok(found)
    }

    /// Reads a file of an allowed skill. The resolved path must stay inside that
    /// skill's folder, so neither `..` nor a symlink can reach anything else. A name the
    /// shared library holds is read from the library; a local skill is only read when the
    /// library does not hold that name.
    pub fn load(&self, name: &str, file: Option<&str>, allowed: &[String]) -> Result<String> {
        if !allowed.iter().any(|skill| skill == name) {
            return Err(CoreError::NotFound(format!("skill {name}")));
        }
        let relative = Path::new(file.unwrap_or("SKILL.md"));
        if !relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        {
            return Err(CoreError::NotFound("skill file".into()));
        }
        let library_holds = self.catalog()?.iter().any(|skill| skill.name == name);
        if !library_holds && self.local.contains(name) {
            return self.local.read(name, file);
        }
        let root = canonicalize(&self.workspace.root()?.join("skills").join(name))
            .map_err(|_| CoreError::NotFound(format!("skill {name}")))?;
        let path = canonicalize(&root.join(relative))
            .map_err(|_| CoreError::NotFound("skill file".into()))?;
        if !path.starts_with(&root) {
            return Err(CoreError::NotFound("skill file".into()));
        }
        Ok(std::fs::read_to_string(path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TempDir, library};
    use orbyn_contracts::WorkspaceSetup;

    async fn fixture() -> (TempDir, SkillService) {
        let dir = TempDir::new();
        let root = library(&dir.path().join("library"));
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        let store = Arc::new(Store::open(&data.join("db"), "e").unwrap());
        let workspace =
            Arc::new(Workspace::new(store.clone(), data.clone(), "u".into(), None).unwrap());
        workspace
            .setup(WorkspaceSetup {
                url: None,
                adopt_path: Some(root.to_string_lossy().into_owned()),
            })
            .await
            .unwrap();
        (dir, SkillService::new(store, workspace, data))
    }

    #[test]
    fn frontmatter_reads_quoted_values_with_colons() {
        assert_eq!(
            parse_frontmatter("---\nname: a\ndescription: \"b: c\"\n---\nbody"),
            Some(("a".into(), "b: c".into()))
        );
        assert_eq!(parse_frontmatter("no frontmatter"), None);
    }

    #[tokio::test]
    async fn identity_is_the_revision_with_a_dirty_flag() {
        let (dir, service) = fixture().await;
        let clean = service.status().await.unwrap();
        assert_eq!(clean.digest.len(), 40);
        assert!(!clean.version.ends_with("-dirty"));
        std::fs::write(dir.path().join("library/AGENTS.md"), "edit").unwrap();
        assert!(service.status().await.unwrap().version.ends_with("-dirty"));
    }

    #[tokio::test]
    async fn each_skill_carries_the_category_the_catalog_gives_it_in_the_catalogs_order() {
        let (_dir, service) = fixture().await;
        let status = service.status().await.unwrap();
        let listed: Vec<(&str, &str)> = status
            .skills
            .iter()
            .map(|skill| (skill.name.as_str(), skill.category.as_str()))
            .collect();
        assert_eq!(
            listed,
            [
                ("how", "Engineering practice"),
                ("social-content", "Social content and visuals"),
            ]
        );
    }

    #[tokio::test]
    async fn default_profile_filters_but_nothing_is_copied() {
        let (dir, service) = fixture().await;
        assert_eq!(service.profiles().unwrap()[0].skills, ["how"]);
        assert!(!dir.path().join("data/skills").exists());
    }

    #[tokio::test]
    async fn local_edits_are_what_the_tools_read() {
        let (dir, service) = fixture().await;
        let allowed = vec!["how".to_owned()];
        std::fs::write(
            dir.path().join("library/skills/how/SKILL.md"),
            "---\nname: how\n---\nedited\n",
        )
        .unwrap();
        assert!(
            service
                .load("how", None, &allowed)
                .unwrap()
                .contains("edited")
        );
    }

    #[tokio::test]
    async fn load_stays_inside_an_allowed_skill() {
        let (_dir, service) = fixture().await;
        let allowed = vec!["how".to_owned()];
        assert!(
            service
                .load("how", None, &allowed)
                .unwrap()
                .contains("# how")
        );
        assert!(service.load("social-content", None, &allowed).is_err());
        assert!(
            service
                .load("how", Some("../../AGENTS.md"), &allowed)
                .is_err()
        );
        assert!(service.load("how", Some("/etc/hosts"), &allowed).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                _dir.path().join("library/AGENTS.md"),
                _dir.path().join("library/skills/how/escape.md"),
            )
            .unwrap();
            assert!(service.load("how", Some("escape.md"), &allowed).is_err());
        }
    }

    #[tokio::test]
    async fn search_stays_inside_the_selection() {
        let (_dir, service) = fixture().await;
        let allowed = vec!["how".to_owned()];
        assert_eq!(service.search("trace", &allowed).unwrap().len(), 1);
        assert!(service.search("posts", &allowed).unwrap().is_empty());
    }

    #[test]
    fn missing_links_fall_back_to_mcp() {
        let native = [
            SkillDelivery::NativeSkillRoot,
            SkillDelivery::McpSkillServer,
        ];
        assert!(route_is_native(&native, true));
        assert!(!route_is_native(&native, false));
        assert!(!route_is_native(&[SkillDelivery::McpSkillServer], true));
    }

    const LOCAL_SKILL: &str = "---
name: redirect-checks
description: \"Check where a redirect or returnTo link goes.\"
---

# Redirect checks

Read this before changing how a redirect is followed.
";

    fn files(root: &Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
        fn walk(root: &Path, at: &Path, out: &mut Vec<(std::path::PathBuf, Vec<u8>)>) {
            for entry in std::fs::read_dir(at).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                let relative = path.strip_prefix(root).unwrap().to_path_buf();
                let file_type = entry.file_type().unwrap();
                if relative.starts_with(".git") || file_type.is_symlink() {
                    continue;
                }
                if file_type.is_dir() {
                    walk(root, &path, out);
                } else {
                    out.push((relative, std::fs::read(&path).unwrap()));
                }
            }
        }
        let mut found = Vec::new();
        if root.exists() {
            walk(root, root, &mut found);
        }
        found.sort();
        found
    }

    #[tokio::test]
    async fn a_local_skill_reaches_the_mcp_reads_and_never_the_library() {
        let (dir, service) = fixture().await;
        let before = service.status().await.unwrap();
        let written = service
            .write_local_skill(LocalSkillWrite::create(LOCAL_SKILL))
            .unwrap();
        assert_eq!(written.name, "redirect-checks");
        assert_eq!(written.local_revision, 1);
        let after = service.status().await.unwrap();
        assert_eq!(after.digest, before.digest);
        assert_eq!(after.version, before.version);
        assert!(!after.version.ends_with("-dirty"));
        assert!(
            after
                .skills
                .iter()
                .all(|skill| skill.name != "redirect-checks")
        );
        assert!(
            service
                .catalog()
                .unwrap()
                .iter()
                .all(|skill| skill.name != "redirect-checks")
        );
        assert!(
            dir.path()
                .join("data/skills-local/redirect-checks/SKILL.md")
                .exists()
        );
        assert!(!dir.path().join("library/skills/redirect-checks").exists());

        let route = service
            .route(&[SkillDelivery::McpSkillServer], None)
            .unwrap();
        assert!(!route.limited);
        let allowed = service.tool_selection(&route).unwrap();
        assert!(
            allowed.contains(&"redirect-checks".to_owned()),
            "{allowed:?}"
        );
        assert!(allowed.contains(&"how".to_owned()), "{allowed:?}");
        let found = service.search("redirect returnTo", &allowed).unwrap();
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0]["name"], "redirect-checks");
        let text = service.load("redirect-checks", None, &allowed).unwrap();
        assert!(text.contains("Read this before changing how a redirect is followed."));
        assert!(text.contains("local_revision: 1"), "{text}");
        // Outside the selection the tools do not answer for it.
        let only_how = vec!["how".to_owned()];
        assert!(service.load("redirect-checks", None, &only_how).is_err());
        assert!(service.search("redirect", &only_how).unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_limited_profile_does_not_get_a_local_skill() {
        let (_dir, service) = fixture().await;
        service
            .write_local_skill(LocalSkillWrite::create(LOCAL_SKILL))
            .unwrap();
        let limited = SkillRoute {
            native: false,
            selection: vec!["how".to_owned()],
            limited: true,
        };
        assert_eq!(service.tool_selection(&limited).unwrap(), ["how"]);
    }

    #[tokio::test]
    async fn a_local_skill_cannot_shadow_a_library_skill() {
        let (dir, service) = fixture().await;
        let error = service
            .write_local_skill(LocalSkillWrite::create(
                "---\nname: how\ndescription: \"Mine now\"\n---\n",
            ))
            .unwrap_err();
        assert_eq!(error.code(), "name_taken", "{error}");
        assert_eq!(error.status(), 409);
        assert!(!dir.path().join("data/skills-local/how").exists());
        let allowed = vec!["how".to_owned()];
        assert!(
            service
                .load("how", None, &allowed)
                .unwrap()
                .contains("# how")
        );
    }

    #[tokio::test]
    async fn a_local_write_leaves_the_library_and_a_target_repository_untouched() {
        let (dir, service) = fixture().await;
        let library = dir.path().join("library");
        let target = library.join("repo/app");
        std::fs::create_dir_all(target.join("src")).unwrap();
        std::fs::write(target.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(target.join("AGENTS.md"), "target rules\n").unwrap();
        let before_library = files(&library);
        let before_target = files(&target);
        let skill = service
            .write_local_skill(LocalSkillWrite::create(LOCAL_SKILL))
            .unwrap();
        service
            .write_local_skill(LocalSkillWrite::edit(
                "redirect-checks",
                LOCAL_SKILL,
                skill.local_revision,
            ))
            .unwrap();
        assert_eq!(files(&library), before_library);
        assert_eq!(files(&target), before_target);
        assert!(!target.join("skills-local").exists());
    }

    #[tokio::test]
    async fn a_library_skill_added_later_wins_over_the_local_one() {
        let (dir, service) = fixture().await;
        service
            .write_local_skill(LocalSkillWrite::create(LOCAL_SKILL))
            .unwrap();
        // The library gains the same name after the local skill was written.
        let root = dir.path().join("library");
        std::fs::create_dir_all(root.join("skills/redirect-checks")).unwrap();
        std::fs::write(
            root.join("skills/redirect-checks/SKILL.md"),
            "---\nname: redirect-checks\ndescription: \"The library one.\"\n---\n\n# Library checks\n",
        )
        .unwrap();
        let catalog_path = root.join("skills/catalog.json");
        let mut catalog: Value =
            serde_json::from_str(&std::fs::read_to_string(&catalog_path).unwrap()).unwrap();
        catalog["skills"]
            .as_array_mut()
            .unwrap()
            .push(json!({ "name": "redirect-checks", "category": "Engineering practice" }));
        std::fs::write(&catalog_path, serde_json::to_string(&catalog).unwrap()).unwrap();

        let route = service
            .route(&[SkillDelivery::McpSkillServer], None)
            .unwrap();
        let allowed = service.tool_selection(&route).unwrap();
        assert_eq!(
            allowed
                .iter()
                .filter(|name| name.as_str() == "redirect-checks")
                .count(),
            1,
            "{allowed:?}"
        );
        let text = service.load("redirect-checks", None, &allowed).unwrap();
        assert!(text.contains("# Library checks"), "{text}");
        assert!(!text.contains("Read this before changing"));
        let found = service.search("redirect", &allowed).unwrap();
        let entries: Vec<&Value> = found
            .iter()
            .filter(|entry| entry.get("name").and_then(Value::as_str) == Some("redirect-checks"))
            .collect();
        assert_eq!(entries.len(), 1, "{found:?}");
        assert_eq!(
            entries[0].get("description").and_then(Value::as_str),
            Some("The library one.")
        );
    }

    #[tokio::test]
    async fn a_library_skill_that_is_off_by_default_is_not_switched_on_by_a_local_one() {
        let (dir, service) = fixture().await;
        let local = "---\nname: social-checks\ndescription: \"A local one.\"\n---\n\n# Local\n";
        service
            .write_local_skill(LocalSkillWrite::create(local))
            .unwrap();
        // The library gains the same name in a category the default selection leaves out.
        let root = dir.path().join("library");
        std::fs::create_dir_all(root.join("skills/social-checks")).unwrap();
        std::fs::write(root.join("skills/social-checks/SKILL.md"), local).unwrap();
        let catalog_path = root.join("skills/catalog.json");
        let mut catalog: Value =
            serde_json::from_str(&std::fs::read_to_string(&catalog_path).unwrap()).unwrap();
        catalog["skills"]
            .as_array_mut()
            .unwrap()
            .push(json!({ "name": "social-checks", "category": "Social" }));
        std::fs::write(&catalog_path, serde_json::to_string(&catalog).unwrap()).unwrap();

        let route = service
            .route(&[SkillDelivery::McpSkillServer], None)
            .unwrap();
        let allowed = service.tool_selection(&route).unwrap();
        assert!(
            !allowed.contains(&"social-checks".to_owned()),
            "{allowed:?}"
        );
    }

    #[tokio::test]
    async fn a_damaged_skills_local_area_does_not_fail_the_run_selection() {
        let (dir, service) = fixture().await;
        // A plain file where the area folder belongs makes reading the area fail.
        let broken = dir.path().join("broken-data");
        std::fs::write(&broken, b"not a folder").unwrap();
        let broken = SkillService::new(service.store.clone(), service.workspace.clone(), broken);
        let route = broken
            .route(&[SkillDelivery::McpSkillServer], None)
            .unwrap();
        assert_eq!(broken.tool_selection(&route).unwrap(), route.selection);
    }

    #[tokio::test]
    async fn a_damaged_skills_local_area_does_not_fail_skill_search() {
        let (dir, service) = fixture().await;
        let broken = dir.path().join("broken-search-data");
        std::fs::write(&broken, b"not a folder").unwrap();
        let broken = SkillService::new(service.store.clone(), service.workspace.clone(), broken);
        let route = broken
            .route(&[SkillDelivery::McpSkillServer], None)
            .unwrap();
        // The local area cannot be read, so the library results are still served.
        let found = broken.search("trace", &route.selection).unwrap();
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0]["name"], "how");
        assert!(
            broken
                .search("redirect", &route.selection)
                .unwrap()
                .is_empty()
        );
    }
}
