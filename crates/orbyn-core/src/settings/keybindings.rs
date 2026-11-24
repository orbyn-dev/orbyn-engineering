//! Keybindings: built-in defaults, then the person's file over them. The file is
//! read leniently, with `//` comments and trailing commas allowed, and one bad entry
//! is reported without losing the others. New defaults reach a person with an
//! existing file, because the file holds only their changes.

use std::path::{Path, PathBuf};

use orbyn_contracts::{KeybindingConflict, KeybindingRule, KeybindingViewRule, KeybindingsView};
use serde_json::Value;

use crate::app::Core;
use crate::error::{CoreError, Result};

const MAX_RULES: usize = 256;
const MAX_FIELD: usize = 256;

/// `(key, command, when)`.
pub(crate) const DEFAULTS: &[(&str, &str, &str)] = &[
    ("mod+b", "sidebar.toggle", ""),
    ("f6", "workspace.focusNext", ""),
    ("shift+f6", "workspace.focusPrevious", ""),
    ("mod+[", "navigation.back", "!terminalFocus"),
    ("mod+]", "navigation.forward", "!terminalFocus"),
    ("mod+j", "terminal.toggle", ""),
    ("mod+alt+b", "rightPanel.toggle", ""),
    ("mod+shift+b", "threadPanel.toggle", "!terminalFocus"),
    (
        "mod+t",
        "rightPanel.new",
        "rightPanelOpen && !terminalFocus",
    ),
    ("mod+d", "terminal.split", "terminalFocus"),
    ("mod+shift+d", "terminal.splitVertical", "terminalFocus"),
    ("mod+n", "terminal.new", "terminalFocus"),
    ("mod+w", "terminal.close", "terminalFocus"),
    ("mod+w", "rightPanel.close", "!terminalFocus"),
    ("mod+shift+t", "view.reopenClosed", ""),
    ("mod+d", "diff.toggle", "!terminalFocus"),
    ("mod+shift+j", "preview.toggle", ""),
    ("mod+r", "preview.refresh", "previewFocus"),
    ("mod+l", "preview.focusUrl", "previewFocus"),
    ("mod+=", "preview.zoomIn", "previewFocus"),
    ("mod+-", "preview.zoomOut", "previewFocus"),
    ("mod+0", "preview.resetZoom", "previewFocus"),
    ("mod+k", "commandPalette.toggle", "!terminalFocus"),
    ("mod+p", "filePicker.toggle", "!terminalFocus"),
    ("mod+shift+f", "projectSearch.toggle", "!terminalFocus"),
    ("mod+u", "usage.open", "!terminalFocus"),
    ("mod+s", "composer.stash", "!terminalFocus"),
    (
        "mod+shift+enter",
        "thread.steerQueuedMessage",
        "!terminalFocus",
    ),
    ("alt+arrowup", "thread.editQueuedMessage", "composerFocus"),
    (
        "mod+enter",
        "composer.sendAlternate",
        "composerFocus && turnRunning",
    ),
    (
        "mod+enter",
        "composer.sendBackground",
        "composerFocus && draftRoute",
    ),
    (
        "mod+alt+enter",
        "composer.sendBackground",
        "composerFocus && draftRoute",
    ),
    (
        "mod+alt+enter",
        "composer.sendAndNewThread",
        "composerFocus && !draftRoute",
    ),
    ("mod+n", "chat.new", "!terminalFocus"),
    ("mod+shift+n", "chat.newLocal", "!terminalFocus"),
    ("mod+alt+n", "chat.newWithoutProject", "!terminalFocus"),
    ("mod+f", "chat.find", "!terminalFocus && !previewFocus"),
    ("mod+shift+m", "modelPicker.toggle", "!terminalFocus"),
    ("mod+shift+e", "composer.effort", "!terminalFocus"),
    ("mod+shift+a", "composer.mode", "!terminalFocus"),
    ("mod+shift+g", "composer.branch", "!terminalFocus"),
    ("mod+shift+x", "composer.workspace", "!terminalFocus"),
    ("mod+shift+l", "composer.previousWorktree", "!terminalFocus"),
    ("mod+alt+a", "theme.select", "!terminalFocus"),
    ("mod+alt+shift+a", "appearance.cycle", "!terminalFocus"),
    ("mod+alt+shift+t", "themeEditor.toggle", ""),
    ("mod+shift+k", "pullRequest.copyNumber", "!terminalFocus"),
    ("mod+o", "editor.openFavorite", ""),
    ("mod+shift+[", "thread.previous", ""),
    ("mod+shift+]", "thread.next", ""),
    ("mod+1", "thread.jump.1", "!terminalFocus"),
    ("mod+2", "thread.jump.2", "!terminalFocus"),
    ("mod+3", "thread.jump.3", "!terminalFocus"),
    ("mod+4", "thread.jump.4", "!terminalFocus"),
    ("mod+5", "thread.jump.5", "!terminalFocus"),
    ("mod+6", "thread.jump.6", "!terminalFocus"),
    ("mod+7", "thread.jump.7", "!terminalFocus"),
    ("mod+8", "thread.jump.8", "!terminalFocus"),
    ("mod+9", "thread.jump.9", "!terminalFocus"),
    ("mod+shift+c", "thread.copyReference", "!terminalFocus"),
    ("mod+shift+s", "thread.settle", "!terminalFocus"),
    ("mod+shift+p", "thread.pin", "!terminalFocus"),
    ("mod+z", "thread.undo", "!terminalFocus && !editableFocus"),
    ("c", "usage.cost", "usagePageOpen"),
    ("t", "usage.tokens", "usagePageOpen"),
    ("l", "usage.limits", "usagePageOpen"),
    ("mod+shift+1", "usage.period.day", "usagePageOpen"),
    ("mod+shift+2", "usage.period.week", "usagePageOpen"),
    ("mod+shift+3", "usage.period.month", "usagePageOpen"),
    ("mod+shift+4", "usage.period.quarter", "usagePageOpen"),
];

fn defaults() -> Vec<KeybindingRule> {
    DEFAULTS
        .iter()
        .map(|(key, command, when)| KeybindingRule {
            key: (*key).to_owned(),
            command: (*command).to_owned(),
            when: (!when.is_empty()).then(|| (*when).to_owned()),
            remove: false,
        })
        .collect()
}

/// Removes `//` comments and trailing commas, leaving strings alone.
fn relax(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if c == '\\' {
                out.extend(chars.next());
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ']' | '}' => {
                let kept = out.trim_end().len();
                if out[..kept].ends_with(',') {
                    out.truncate(kept - 1);
                }
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

fn valid(rule: &KeybindingRule) -> bool {
    !rule.key.trim().is_empty()
        && !rule.command.trim().is_empty()
        && rule.key.len() <= MAX_FIELD
        && rule.command.len() <= MAX_FIELD
        && rule
            .when
            .as_ref()
            .is_none_or(|when| when.len() <= MAX_FIELD)
}

/// The rules a file holds, and what was wrong with the rest of it.
fn parse(text: &str) -> (Vec<KeybindingRule>, Vec<String>) {
    let items = match serde_json::from_str::<Value>(&relax(text)) {
        Ok(Value::Array(items)) => items,
        Ok(_) => {
            return (
                Vec::new(),
                vec!["The file must hold a list of rules.".into()],
            );
        }
        Err(error) => return (Vec::new(), vec![format!("The file is not valid: {error}")]),
    };
    let mut rules = Vec::new();
    let mut issues = Vec::new();
    for (index, item) in items.into_iter().take(MAX_RULES).enumerate() {
        match serde_json::from_value::<KeybindingRule>(item) {
            Ok(rule) if valid(&rule) => rules.push(rule),
            _ => issues.push(format!("Rule {} was skipped.", index + 1)),
        }
    }
    (rules, issues)
}

/// The file's rules over the defaults, each marked with whether it is the person's own.
/// A later rule for one key and context wins, and `remove` takes a rule out.
fn merged(own: &[KeybindingRule]) -> Vec<(KeybindingRule, bool)> {
    let mut rules: Vec<(KeybindingRule, bool)> =
        defaults().into_iter().map(|rule| (rule, false)).collect();
    for rule in own {
        if rule.remove {
            rules.retain(|(known, _)| !(known.key == rule.key && known.command == rule.command));
            continue;
        }
        rules.retain(|(known, _)| !(known.command == rule.command && known.when == rule.when));
        rules.retain(|(known, _)| !(known.key == rule.key && known.when == rule.when));
        rules.push((rule.clone(), true));
    }
    rules
}

/// The built-in keys of a command in a context, in table order.
fn default_keys(command: &str, when: Option<&str>) -> Vec<String> {
    let when = when.filter(|when| !when.trim().is_empty());
    DEFAULTS
        .iter()
        .filter(|(_, known_command, known_when)| {
            *known_command == command
                && (!known_when.trim().is_empty()).then_some(*known_when) == when
        })
        .map(|(key, _, _)| (*key).to_owned())
        .collect()
}

/// Report exact-context collisions and unconditional bindings that overlap a context.
/// Distinct nonempty contexts are left separate because they may be disjoint.
fn conflicts(rules: &[KeybindingRule]) -> Vec<KeybindingConflict> {
    let mut found: Vec<KeybindingConflict> = Vec::new();
    for rule in rules {
        let key = crate::account::settings_record::normalize_key(&rule.key);
        let when = rule.when.as_ref().filter(|when| !when.trim().is_empty());
        match found
            .iter_mut()
            .find(|known| known.key == key && known.when.as_ref() == when)
        {
            Some(known) => known.commands.push(rule.command.clone()),
            None => found.push(KeybindingConflict {
                key,
                when: when.cloned(),
                commands: vec![rule.command.clone()],
            }),
        }
    }
    for conflict in &mut found {
        if conflict
            .when
            .as_deref()
            .is_some_and(|when| !when.trim().is_empty())
        {
            let mut unconditional: Vec<String> = rules
                .iter()
                .filter(|rule| {
                    crate::account::settings_record::normalize_key(&rule.key) == conflict.key
                        && rule
                            .when
                            .as_deref()
                            .is_none_or(|when| when.trim().is_empty())
                })
                .map(|rule| rule.command.clone())
                .collect();
            unconditional.append(&mut conflict.commands);
            conflict.commands = unconditional;
        }
    }
    for conflict in &mut found {
        let mut seen = std::collections::HashSet::new();
        conflict
            .commands
            .retain(|command| seen.insert(command.clone()));
    }
    found.retain(|conflict| conflict.commands.len() > 1);
    found
}

fn read(path: &Path) -> (Vec<KeybindingRule>, Vec<String>) {
    std::fs::read_to_string(path).map_or_else(|_| (Vec::new(), Vec::new()), |text| parse(&text))
}

impl Core {
    fn keybindings_path(&self) -> PathBuf {
        self.config.data_dir.join("keybindings.json")
    }

    pub fn keybindings(&self) -> KeybindingsView {
        let path = self.keybindings_path();
        let (own, issues) = read(&path);
        let rules = merged(&own);
        let plain: Vec<KeybindingRule> = rules.iter().map(|(rule, _)| rule).cloned().collect();
        KeybindingsView {
            path: path.to_string_lossy().into_owned(),
            conflicts: conflicts(&plain),
            rules: rules
                .into_iter()
                .map(|(rule, custom)| {
                    let default_keys = default_keys(&rule.command, rule.when.as_deref());
                    KeybindingViewRule {
                        key: rule.key,
                        command: rule.command,
                        when: rule.when,
                        remove: false,
                        custom: Some(custom),
                        default_keys: Some(default_keys),
                    }
                })
                .collect(),
            issues,
        }
    }

    /// Adds or changes a rule in the file, by rename so a crash cannot truncate it.
    /// A file that does not parse is left for the person to fix.
    pub fn save_keybinding(&self, rule: &KeybindingRule) -> Result<KeybindingsView> {
        let _writer = self
            .keybinding_writer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !valid(rule) {
            return Err(CoreError::Invalid(
                "A rule needs a key and a command.".into(),
            ));
        }
        let path = self.keybindings_path();
        let (mut own, issues) = read(&path);
        if !issues.is_empty() {
            return Err(CoreError::Invalid(
                "Fix the keybindings file before changing it here.".into(),
            ));
        }
        // What this rule displaces leaves a removal marker for the account's record, or
        // another device would keep the old rule beside the new one.
        let key = crate::account::settings_record::normalize_key(&rule.key);
        let same_key = |known: &KeybindingRule| {
            crate::account::settings_record::normalize_key(&known.key) == key
        };
        let displaced: Vec<KeybindingRule> = own
            .iter()
            .filter(|known| {
                (known.command == rule.command && known.when == rule.when)
                    || (same_key(known) && known.command == rule.command)
            })
            .filter(|known| !(same_key(known) && known.when == rule.when))
            .map(|known| KeybindingRule {
                remove: true,
                ..known.clone()
            })
            .collect();
        own.retain(|known| {
            !(known.command == rule.command && known.when == rule.when)
                && !(same_key(known) && known.command == rule.command)
        });
        own.push(rule.clone());
        write_rules(&path, &own)?;
        // Queued for sync when the account is on; the file is already saved, so a
        // failure here never fails the person's edit.
        let staged: Vec<_> = displaced
            .iter()
            .chain(std::iter::once(rule))
            .map(crate::account::settings_record::stage_keybinding)
            .collect();
        if let Err(error) = self
            .store
            .account_stage(crate::account::settings_record::ZONE, &staged)
        {
            tracing::warn!(%error, "could not queue a keybinding for sync");
        }
        Ok(self.keybindings())
    }

    /// Drops the person's own rules for a command and context, so the built-in binding
    /// shows again. A hidden default stays hidden.
    pub fn reset_keybinding(&self, command: &str, when: Option<&str>) -> Result<KeybindingsView> {
        let _writer = self
            .keybinding_writer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if command.trim().is_empty() || command.len() > MAX_FIELD {
            return Err(CoreError::Invalid("A reset needs a command.".into()));
        }
        let path = self.keybindings_path();
        let (mut own, issues) = read(&path);
        if !issues.is_empty() {
            return Err(CoreError::Invalid(
                "Fix the keybindings file before changing it here.".into(),
            ));
        }
        let when = when.filter(|when| !when.trim().is_empty());
        let same = |rule: &KeybindingRule| {
            !rule.remove
                && rule.command == command
                && rule.when.as_deref().filter(|when| !when.trim().is_empty()) == when
        };
        let dropped: Vec<KeybindingRule> = own
            .iter()
            .filter(|rule| same(*rule))
            .map(|rule| KeybindingRule {
                remove: true,
                ..rule.clone()
            })
            .collect();
        if dropped.is_empty() {
            return Ok(self.keybindings());
        }
        own.retain(|rule| !same(rule));
        write_rules(&path, &own)?;
        // Queued for sync when the account is on; the file is already saved, so a
        // failure here never fails the person's reset.
        let staged: Vec<_> = dropped
            .iter()
            .map(crate::account::settings_record::stage_keybinding)
            .collect();
        if let Err(error) = self
            .store
            .account_stage(crate::account::settings_record::ZONE, &staged)
        {
            tracing::warn!(%error, "could not queue a keybinding for sync");
        }
        Ok(self.keybindings())
    }
}

/// Saves the file by rename, so a crash cannot truncate it.
fn write_rules(path: &Path, rules: &[KeybindingRule]) -> Result<()> {
    let partial = path.with_extension("json.partial");
    std::fs::write(&partial, serde_json::to_vec_pretty(rules)?)?;
    std::fs::rename(&partial, path)?;
    Ok(())
}

/// The rules the person's own file holds, for the account to queue on first sync.
pub(crate) fn own_rules(path: &Path) -> Vec<KeybindingRule> {
    read(path).0
}

/// Writes rules that arrived from another device, and queues nothing. A rule replaces
/// the one with the same key and context, and one already in the file is left alone, so
/// applying the same record twice changes nothing. A file that does not parse is left
/// for the person to fix. Returns whether the file changed.
pub(crate) fn write_synced_rules(
    path: &Path,
    writer: &std::sync::Mutex<()>,
    rules: &[KeybindingRule],
) -> Result<bool> {
    let _writer = writer
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (mut own, issues) = read(path);
    if !issues.is_empty() {
        return Err(CoreError::Invalid(
            "Fix the keybindings file before changing it here.".into(),
        ));
    }
    let before = own.len();
    let mut changed = false;
    // The same chord however it is spelled: `Mod+Y` and `mod+y` are one rule.
    let same_chord = |a: &KeybindingRule, b: &KeybindingRule| {
        crate::account::settings_record::normalize_key(&a.key)
            == crate::account::settings_record::normalize_key(&b.key)
            && a.when == b.when
    };
    for rule in rules.iter().filter(|rule| valid(rule)) {
        if own.iter().any(|known| {
            same_chord(known, rule) && known.command == rule.command && known.remove == rule.remove
        }) {
            continue;
        }
        own.retain(|known| !same_chord(known, rule));
        own.push(rule.clone());
        changed = true;
    }
    if changed || own.len() != before {
        write_rules(path, &own)?;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(key: &str, command: &str, when: Option<&str>) -> KeybindingRule {
        KeybindingRule {
            key: key.into(),
            command: command.into(),
            when: when.map(str::to_owned),
            remove: false,
        }
    }

    fn merged_rules(own: &[KeybindingRule]) -> Vec<KeybindingRule> {
        merged(own).into_iter().map(|(rule, _)| rule).collect()
    }

    fn core_with_account() -> (crate::testing::TempDir, Core) {
        use crate::account::settings_record::ZONE;
        use crate::{CoreConfig, testing::TempDir};
        let dir = TempDir::new();
        let mut config = CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
        config.engines.lookup_latest = false;
        let core = Core::open(config).unwrap();
        core.store
            .account_enable(ZONE, &[], false, Some("test-device"))
            .unwrap();
        (dir, core)
    }

    #[test]
    fn saving_equivalent_chord_in_another_context_replaces_and_stages_the_old_rule() {
        use crate::account::settings_record::{ZONE, keybinding_name};
        use crate::{CoreConfig, testing::TempDir};
        let dir = TempDir::new();
        let mut config = CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
        config.engines.lookup_latest = false;
        let core = Core::open(config).unwrap();
        core.store
            .account_enable(ZONE, &[], false, Some("test-device"))
            .unwrap();
        let old = rule("Mod + Y", "example.action", Some("editor"));
        let other = rule("mod+y", "other.action", Some("browser"));
        let other_chord = rule("alt+z", "example.action", Some("terminal"));
        for value in [&old, &other, &other_chord] {
            core.save_keybinding(value).unwrap();
        }
        let replacement = rule("mod+y", "example.action", Some("sidebar"));
        core.save_keybinding(&replacement).unwrap();
        let (own, issues) = read(&core.keybindings_path());
        assert!(issues.is_empty());
        assert_eq!(own.len(), 3, "{own:?}");
        assert!(own.iter().any(|r| r.key == replacement.key
            && r.when == replacement.when
            && r.command == replacement.command));
        assert!(
            own.iter()
                .any(|r| r.key == other.key && r.when == other.when && r.command == other.command)
        );
        assert!(own.iter().any(|r| r.key == other_chord.key
            && r.when == other_chord.when
            && r.command == other_chord.command));
        let record = core.store.account_record(ZONE).unwrap();
        let previous = &record[&keybinding_name(&old.key, old.when.as_deref())].value;
        assert_eq!(previous["remove"], true);
        let current =
            &record[&keybinding_name(&replacement.key, replacement.when.as_deref())].value;
        assert_eq!(current["remove"], false);
        assert_eq!(current["command"], replacement.command);
        let same_field = rule("MOD + Y", "example.action", Some("sidebar"));
        core.save_keybinding(&same_field).unwrap();
        let record = core.store.account_record(ZONE).unwrap();
        let current = &record[&keybinding_name(&same_field.key, same_field.when.as_deref())].value;
        assert_eq!(current["remove"], false);
        assert_eq!(current["key"], "MOD + Y");
        let removed = KeybindingRule {
            remove: true,
            ..same_field
        };
        core.save_keybinding(&removed).unwrap();
        let record = core.store.account_record(ZONE).unwrap();
        assert_eq!(
            record[&keybinding_name(&removed.key, removed.when.as_deref())].value["remove"],
            true
        );
    }

    #[test]
    fn reset_restores_the_built_in_binding_and_queues_a_removal() {
        use crate::account::settings_record::{ZONE, keybinding_name};
        let (_dir, core) = core_with_account();
        core.save_keybinding(&rule("mod+y", "sidebar.toggle", None))
            .unwrap();
        let custom = core
            .keybindings()
            .rules
            .into_iter()
            .find(|r| r.key == "mod+y" && r.command == "sidebar.toggle")
            .expect("the rebinding is in the view");
        assert_eq!(custom.custom, Some(true));
        assert_eq!(custom.default_keys, Some(vec!["mod+b".to_owned()]));
        assert!(
            !core
                .keybindings()
                .rules
                .iter()
                .any(|r| r.key == "mod+b" && r.command == "sidebar.toggle")
        );

        let view = core.reset_keybinding("sidebar.toggle", None).unwrap();
        let restored = view
            .rules
            .iter()
            .find(|r| r.key == "mod+b" && r.command == "sidebar.toggle")
            .expect("the built-in binding is back");
        assert_eq!(restored.custom, Some(false));
        assert_eq!(restored.default_keys, Some(vec!["mod+b".to_owned()]));
        assert!(!view.rules.iter().any(|r| r.key == "mod+y"));

        let (own, issues) = read(&core.keybindings_path());
        assert!(issues.is_empty());
        assert!(
            !own.iter()
                .any(|r| r.command == "sidebar.toggle" && !r.remove)
        );
        let record = core.store.account_record(ZONE).unwrap();
        let removed = &record[&keybinding_name("mod+y", None)].value;
        assert_eq!(removed["remove"], true);
        assert_eq!(removed["command"], "sidebar.toggle");
    }

    #[test]
    fn reset_keeps_a_hidden_default_hidden() {
        let (_dir, core) = core_with_account();
        let mut hidden = rule("mod+b", "sidebar.toggle", None);
        hidden.remove = true;
        core.save_keybinding(&hidden).unwrap();
        core.save_keybinding(&rule("mod+y", "thread.pin", Some("!terminalFocus")))
            .unwrap();
        let view = core
            .reset_keybinding("thread.pin", Some("!terminalFocus"))
            .unwrap();
        assert!(
            !view
                .rules
                .iter()
                .any(|r| r.key == "mod+b" && r.command == "sidebar.toggle")
        );
        let pinned = view
            .rules
            .iter()
            .find(|r| r.command == "thread.pin")
            .expect("thread.pin is bound");
        assert_eq!(pinned.key, "mod+shift+p");
        assert_eq!(pinned.custom, Some(false));
        assert!(!view.rules.iter().any(|r| r.key == "mod+y"));
    }

    #[test]
    fn reset_of_a_command_with_no_custom_rule_changes_nothing() {
        let (_dir, core) = core_with_account();
        let before = core.keybindings();
        let after = core.reset_keybinding("sidebar.toggle", None).unwrap();
        assert_eq!(after, before);
        assert!(!core.keybindings_path().exists());
    }

    #[test]
    fn a_bound_command_with_no_built_in_has_no_default_keys() {
        let (_dir, core) = core_with_account();
        core.save_keybinding(&rule("mod+y", "my.custom.command", None))
            .unwrap();
        let custom = core
            .keybindings()
            .rules
            .into_iter()
            .find(|r| r.key == "mod+y" && r.command == "my.custom.command")
            .expect("the custom rule is in the view");
        assert_eq!(custom.custom, Some(true));
        assert_eq!(custom.default_keys, Some(Vec::new()));
    }

    #[test]
    fn the_defaults_have_no_conflicts() {
        assert!(
            conflicts(&defaults()).is_empty(),
            "{:?}",
            conflicts(&defaults())
        );
    }

    #[test]
    fn nine_jumps_are_bound_and_each_can_be_rebound_alone() {
        let rules = defaults();
        for digit in 1..=9 {
            let command = format!("thread.jump.{digit}");
            let found: Vec<_> = rules.iter().filter(|r| r.command == command).collect();
            assert_eq!(found.len(), 1, "{command}");
            assert_eq!(found[0].key, format!("mod+{digit}"));
        }
        let moved = merged_rules(&[rule("alt+3", "thread.jump.3", Some("!terminalFocus"))]);
        assert!(!moved.iter().any(|r| r.key == "mod+3"));
        assert!(moved.iter().any(|r| r.key == "mod+4"));
    }

    #[test]
    fn a_lenient_file_keeps_good_rules_and_names_bad_ones() {
        let text = r#"[
            // mine
            { "key": "mod+x", "command": "a // not a comment" },
            { "key": "", "command": "b" },
            { "nonsense": true },
        ]"#;
        let (rules, issues) = parse(text);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].command, "a // not a comment");
        assert_eq!(issues.len(), 2);
        assert_eq!(parse("{ nope").1.len(), 1);
    }

    #[test]
    fn repeated_command_in_overlapping_contexts_is_not_a_conflict() {
        let rules = [
            rule("mod+q", "same", None),
            rule("mod+q", "same", Some("")),
            rule("mod+q", "same", Some("!terminalFocus")),
        ];
        assert!(conflicts(&rules).is_empty());
        let mut with_other = rules.to_vec();
        with_other.push(rule("mod+q", "other", Some("!terminalFocus")));
        let clashes = conflicts(&with_other);
        assert_eq!(clashes.len(), 1);
        assert_eq!(clashes[0].commands, ["same", "other"]);
    }

    #[test]
    fn unconditional_context_variants_collide() {
        let rules = [
            rule("mod+q", "one", None),
            rule("mod+q", "two", Some("")),
            rule("mod+q", "three", Some("  ")),
        ];
        let clashes = conflicts(&merged_rules(&rules));
        let clash = clashes
            .iter()
            .find(|clash| clash.key == "mod+q")
            .expect("unconditional rules overlap");
        assert_eq!(clash.when, None);
        assert_eq!(clash.commands, ["one", "two", "three"]);
    }

    #[test]
    fn differently_spelled_keys_conflict_under_the_normalised_key() {
        let rules = [rule("Mod+K", "a.one", None), rule("mod+k ", "a.two", None)];
        let clashes = conflicts(&rules);
        assert_eq!(clashes.len(), 1);
        assert_eq!(clashes[0].key, "mod+k");
        assert_eq!(clashes[0].commands, ["a.one", "a.two"]);
        let mixed = [
            rule("mod+k", "a.one", None),
            rule("MOD+K", "a.two", Some("!terminalFocus")),
        ];
        let clashes = conflicts(&mixed);
        assert_eq!(clashes.len(), 1);
        assert_eq!(clashes[0].key, "mod+k");
        assert_eq!(clashes[0].when.as_deref(), Some("!terminalFocus"));
        assert_eq!(clashes[0].commands, ["a.one", "a.two"]);
    }

    #[test]
    fn rebinding_sidebar_to_palette_key_reports_overlap() {
        let rules = merged_rules(&[rule("mod+k", "sidebar.toggle", None)]);
        let clashes = conflicts(&rules);
        let clash = clashes.iter().find(|clash| clash.key == "mod+k").unwrap();
        assert_eq!(clash.when.as_deref(), Some("!terminalFocus"));
        assert!(
            clash
                .commands
                .iter()
                .any(|command| command == "sidebar.toggle")
        );
        assert!(
            clash
                .commands
                .iter()
                .any(|command| command == "commandPalette.toggle")
        );
    }

    #[test]
    fn the_file_rebinds_removes_and_reports_conflicts() {
        let rules = merged_rules(&[rule("mod+y", "sidebar.toggle", None)]);
        assert!(
            rules
                .iter()
                .any(|r| r.key == "mod+y" && r.command == "sidebar.toggle")
        );
        assert!(!rules.iter().any(|r| r.key == "mod+b"));
        let gone = KeybindingRule {
            remove: true,
            ..rule("mod+j", "terminal.toggle", None)
        };
        assert!(
            !merged_rules(&[gone])
                .iter()
                .any(|r| r.command == "terminal.toggle")
        );
        let clash = [rule("mod+q", "one", None), rule("mod+q", "two", Some("x"))];
        assert_eq!(conflicts(&merged_rules(&clash))[0].commands, ["one", "two"]);
        let disjoint = [
            rule("mod+q", "one", Some("terminalFocus")),
            rule("mod+q", "two", Some("!terminalFocus")),
        ];
        assert!(conflicts(&merged_rules(&disjoint)).is_empty());
        let blank = [
            rule("mod+q", "one", Some("  ")),
            rule("mod+q", "two", Some("!terminalFocus")),
        ];
        assert_eq!(conflicts(&merged_rules(&blank))[0].commands, ["one", "two"]);
        let same = [rule("a", "one", None), rule("a", "two", None)];
        assert_eq!(conflicts(&same)[0].commands, ["one", "two"]);
    }
}
