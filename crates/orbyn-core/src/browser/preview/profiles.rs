//! Named browser profiles: a folder each, kept when a tab ends, and held by one tab at a time.

use super::lock;
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::util::new_id;
use orbyn_contracts::BrowserProfile;
use serde_json::{Value, json};
use std::path::PathBuf;

pub(super) const PROFILE_NAME_LIMIT: usize = 64;

/// The name rule create and rename share, so both refuse with the same sentence.
fn checked_profile_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > PROFILE_NAME_LIMIT {
        return Err(CoreError::Invalid(format!(
            "A profile name is 1 to {PROFILE_NAME_LIMIT} characters."
        )));
    }
    Ok(name)
}

impl Core {
    pub(super) fn profile_folder(&self, id: &str) -> PathBuf {
        self.config.data_dir.join("browser-profiles").join(id)
    }

    pub(super) fn stored_profiles(&self) -> Result<Vec<(String, String)>> {
        let list = self.preference("preview_profiles", None)?;
        Ok(list
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .filter_map(|entry| {
                let text = |key: &str| entry.get(key)?.as_str().map(str::to_owned);
                Some((text("id")?, text("name")?))
            })
            .collect())
    }

    pub(super) fn profile_open(&self, id: &str) -> bool {
        lock(&self.previews.tabs)
            .values()
            .any(|tab| lock(&tab.info).profile.as_deref() == Some(id))
    }

    /// A browser locks its profile, so a named one holds one tab at a time.
    pub(super) fn claim_profile(&self, id: &str) -> Result<PathBuf> {
        if !self.stored_profiles()?.iter().any(|(known, _)| known == id) {
            return Err(CoreError::NotFound("browser profile".into()));
        }
        if self.profile_open(id) {
            return Err(CoreError::Invalid(
                "That profile is open in another tab.".into(),
            ));
        }
        Ok(self.profile_folder(id))
    }

    pub fn browser_profiles(&self) -> Result<Vec<BrowserProfile>> {
        Ok(self
            .stored_profiles()?
            .into_iter()
            .map(|(id, name)| BrowserProfile {
                in_use: self.profile_open(&id),
                id,
                name,
            })
            .collect())
    }

    pub(super) fn save_profiles(&self, profiles: &[(String, String)]) -> Result<()> {
        let list: Vec<Value> = profiles
            .iter()
            .map(|(id, name)| json!({ "id": id, "name": name }))
            .collect();
        let values =
            std::collections::BTreeMap::from([("preview_profiles".to_owned(), json!(list))]);
        self.set_preferences(None, &values)
    }

    /// Only the stored name changes, so a profile open in a tab can be renamed.
    pub fn rename_browser_profile(&self, id: &str, name: &str) -> Result<BrowserProfile> {
        let name = checked_profile_name(name)?;
        let mut profiles = self.stored_profiles()?;
        let index = profiles
            .iter()
            .position(|(known, _)| known == id)
            .ok_or_else(|| CoreError::NotFound("browser profile".into()))?;
        name.clone_into(&mut profiles[index].1);
        self.save_profiles(&profiles)?;
        Ok(BrowserProfile {
            id: id.to_owned(),
            name: name.to_owned(),
            in_use: self.profile_open(id),
        })
    }

    pub fn create_browser_profile(&self, name: &str) -> Result<BrowserProfile> {
        let name = checked_profile_name(name)?;
        let mut profiles = self.stored_profiles()?;
        let id = new_id();
        profiles.push((id.clone(), name.to_owned()));
        self.save_profiles(&profiles)?;
        Ok(BrowserProfile {
            id,
            name: name.to_owned(),
            in_use: false,
        })
    }

    pub(super) fn release_profile(&self, id: &str) -> Result<()> {
        if !self.stored_profiles()?.iter().any(|(known, _)| known == id) {
            return Err(CoreError::NotFound("browser profile".into()));
        }
        if self.profile_open(id) {
            return Err(CoreError::Invalid(
                "Close the tab using this profile first.".into(),
            ));
        }
        match std::fs::remove_dir_all(self.profile_folder(id)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        }
    }

    /// Empties a profile: cookies, storage and history go, and the name stays.
    pub fn clear_browser_profile(&self, id: &str) -> Result<()> {
        self.release_profile(id)
    }

    pub fn delete_browser_profile(&self, id: &str) -> Result<()> {
        self.release_profile(id)?;
        let mut profiles = self.stored_profiles()?;
        profiles.retain(|(known, _)| known != id);
        self.save_profiles(&profiles)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::CoreConfig;
    use crate::testing::TempDir;

    fn core(dir: &TempDir) -> Core {
        Core::open(CoreConfig::new(
            dir.path().join("data"),
            "http://127.0.0.1:1".into(),
        ))
        .unwrap()
    }

    /// A profile folder with a file in it, so a rename that moved the folder would show.
    fn folder_with_note(core: &Core, id: &str) -> PathBuf {
        let folder = core.profile_folder(id);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("note"), "kept").unwrap();
        folder
    }

    #[test]
    fn rename_changes_the_stored_name_and_returns_it() {
        let dir = TempDir::new();
        let core = core(&dir);
        let profile = core.create_browser_profile(" Work ").unwrap();
        let renamed = core
            .rename_browser_profile(&profile.id, "  Research  ")
            .unwrap();
        assert_eq!(renamed.id, profile.id);
        assert_eq!(renamed.name, "Research");
        assert!(!renamed.in_use);
        assert_eq!(core.browser_profiles().unwrap()[0].name, "Research");
    }

    #[test]
    fn rename_keeps_the_listing_order_and_accepts_a_name_another_profile_has() {
        let dir = TempDir::new();
        let core = core(&dir);
        let first = core.create_browser_profile("Work").unwrap();
        let second = core.create_browser_profile("Home").unwrap();
        core.rename_browser_profile(&first.id, "Home").unwrap();
        let listed = core.browser_profiles().unwrap();
        assert_eq!(
            listed
                .iter()
                .map(|profile| profile.id.as_str())
                .collect::<Vec<_>>(),
            vec![first.id.as_str(), second.id.as_str()]
        );
        assert_eq!(
            listed
                .iter()
                .map(|profile| profile.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Home", "Home"]
        );
    }

    #[test]
    fn a_profile_name_is_one_to_sixty_four_characters_for_create_and_rename() {
        let dir = TempDir::new();
        let core = core(&dir);
        let profile = core.create_browser_profile("Work").unwrap();
        let sixty_four = "x".repeat(PROFILE_NAME_LIMIT);
        let sixty_five = "x".repeat(PROFILE_NAME_LIMIT + 1);
        let sentence = format!("A profile name is 1 to {PROFILE_NAME_LIMIT} characters.");
        for name in ["", "   ", sixty_five.as_str()] {
            match core.rename_browser_profile(&profile.id, name) {
                Err(CoreError::Invalid(message)) => assert_eq!(message, sentence),
                other => panic!("rename accepted {name:?}: {other:?}"),
            }
        }
        match core.create_browser_profile(&sixty_five) {
            Err(CoreError::Invalid(message)) => assert_eq!(message, sentence),
            other => panic!("create accepted a 65-character name: {other:?}"),
        }
        assert_eq!(
            core.rename_browser_profile(&profile.id, &sixty_four)
                .unwrap()
                .name,
            sixty_four
        );
    }

    #[test]
    fn renaming_an_unknown_profile_is_not_found() {
        let dir = TempDir::new();
        let core = core(&dir);
        match core.rename_browser_profile("missing", "Work") {
            Err(CoreError::NotFound(what)) => assert_eq!(what, "browser profile"),
            other => panic!("an unknown profile was not refused: {other:?}"),
        }
    }

    #[tokio::test]
    async fn renaming_an_open_profile_keeps_it_in_use_and_the_folder_in_place() {
        let dir = TempDir::new();
        let core = core(&dir);
        let profile = core.create_browser_profile("Work").unwrap();
        let folder = folder_with_note(&core, &profile.id);
        let tab = super::super::tests::fixture_profile_tab(&profile.id).await;
        lock(&core.previews.tabs).insert("rename-open-tab".into(), tab);
        assert!(core.browser_profiles().unwrap()[0].in_use);

        let renamed = core
            .rename_browser_profile(&profile.id, "Research")
            .unwrap();
        assert_eq!(renamed.name, "Research");
        assert!(renamed.in_use, "a rename does not close the tab");
        assert!(core.browser_profiles().unwrap()[0].in_use);
        assert!(folder.exists(), "the profile folder does not move");
        assert_eq!(
            std::fs::read_to_string(folder.join("note")).unwrap(),
            "kept",
            "the folder's contents do not change"
        );

        lock(&core.previews.tabs).clear();
        assert!(!core.browser_profiles().unwrap()[0].in_use);
        assert!(folder.exists());
    }
}
