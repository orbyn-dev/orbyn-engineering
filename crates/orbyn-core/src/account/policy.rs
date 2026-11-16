//! What a peer may make this device do, and nothing more
//! ([No escalation across devices](../../../../docs/account.md#no-escalation-across-devices),
//! [threat model 5b](../../../../docs/account-threat-model.md)). Every rule here only
//! narrows: a value that fails is kept and passed on, never applied.

use orbyn_contracts::KeybindingRule;

/// The commands a synced keybinding may bind: navigation and view commands only. A
/// command that approves or denies an agent action, changes permissions, or confirms,
/// sends or runs something is absent, and so is anything not listed.
const KEYBINDING_COMMANDS: &[&str] = &[
    "sidebar.toggle",
    "workspace.focusNext",
    "workspace.focusPrevious",
    "navigation.back",
    "navigation.forward",
    "terminal.toggle",
    "rightPanel.toggle",
    "rightPanel.close",
    "threadPanel.toggle",
    "view.reopenClosed",
    "diff.toggle",
    "preview.toggle",
    "preview.refresh",
    "preview.focusUrl",
    "preview.zoomIn",
    "preview.zoomOut",
    "preview.resetZoom",
    "commandPalette.toggle",
    "filePicker.toggle",
    "projectSearch.toggle",
    "usage.open",
    "usage.cost",
    "usage.tokens",
    "usage.limits",
    "usage.period.day",
    "usage.period.week",
    "usage.period.month",
    "usage.period.quarter",
    "chat.find",
    "theme.select",
    "appearance.cycle",
    "themeEditor.toggle",
    "thread.previous",
    "thread.next",
    "thread.jump.1",
    "thread.jump.2",
    "thread.jump.3",
    "thread.jump.4",
    "thread.jump.5",
    "thread.jump.6",
    "thread.jump.7",
    "thread.jump.8",
    "thread.jump.9",
];

/// The bare keys a dialog answers by default, such as an approval or "Confirm it's
/// you". A synced rule may not bind them, so a peer cannot steal the key that answers
/// a prompt. The web app has no per-dialog key handler today, so this is the standard
/// set of dialog keys; the Designer's frames may add to it.
const DIALOG_KEYS: &[&str] = &[
    "enter",
    "return",
    "escape",
    "esc",
    "space",
    "tab",
    "shift+tab",
];

/// Whether a rule from a peer may be applied here.
pub fn keybinding_allowed(rule: &KeybindingRule) -> bool {
    KEYBINDING_COMMANDS.contains(&rule.command.as_str())
        && !DIALOG_KEYS.contains(&super::settings_record::normalize_key(&rule.key).as_str())
}

/// Whether `prefix` is safe as a git ref part: it is passed to git as an argument, and
/// it must not start a flag, hold a character git refuses or escape its folder. It is
/// stricter than `git check-ref-format`: ASCII letters, digits and `. _ - /` only.
pub fn branch_prefix_is_ref_part(prefix: &str) -> bool {
    if prefix.is_empty() || prefix.len() > 100 {
        return false;
    }
    let plain = prefix
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/'));
    plain
        && !prefix.contains("..")
        && !prefix.ends_with('/')
        && !prefix.ends_with('.')
        && prefix.split('/').all(|part| {
            part.bytes()
                .next()
                .is_some_and(|b| b.is_ascii_alphanumeric())
                && !part.ends_with(".lock")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(key: &str, command: &str) -> KeybindingRule {
        KeybindingRule {
            key: key.into(),
            command: command.into(),
            when: None,
            remove: false,
        }
    }

    #[test]
    fn a_navigation_command_on_an_ordinary_key_is_allowed() {
        assert!(keybinding_allowed(&rule("mod+y", "sidebar.toggle")));
        assert!(keybinding_allowed(&rule("alt+3", "thread.jump.3")));
    }

    #[test]
    fn a_command_that_approves_changes_permissions_confirms_or_runs_is_not() {
        for command in [
            "approval.approve",
            "approval.deny",
            "approval.allowAlways",
            "permissions.setMode",
            "composer.mode",
            "composer.sendAlternate",
            "composer.sendBackground",
            "composer.sendAndNewThread",
            "thread.steerQueuedMessage",
            "thread.settle",
            "terminal.new",
            "terminal.split",
            "editor.openFavorite",
            "account.confirmYou",
            "",
        ] {
            assert!(!keybinding_allowed(&rule("mod+y", command)), "{command}");
        }
    }

    #[test]
    fn the_keys_of_a_dialog_cannot_be_bound_however_they_are_spelled() {
        for key in [
            "enter",
            "Enter",
            " ESCAPE ",
            "esc",
            "space",
            "tab",
            "shift + tab",
            "return",
        ] {
            assert!(!keybinding_allowed(&rule(key, "sidebar.toggle")), "{key}");
        }
        // With a modifier it is a different chord.
        assert!(keybinding_allowed(&rule("mod+enter", "sidebar.toggle")));
    }

    #[test]
    fn every_allowlisted_command_is_a_built_in_one_and_no_acting_command_is_listed() {
        let built_in: Vec<&str> = crate::settings::keybindings::DEFAULTS
            .iter()
            .map(|(_, command, _)| *command)
            .collect();
        for command in KEYBINDING_COMMANDS {
            assert!(
                built_in.contains(command),
                "{command} is not a built-in command"
            );
            for acting in [
                "composer.",
                "terminal.new",
                "terminal.split",
                "terminal.close",
                "thread.settle",
                "thread.pin",
                "thread.undo",
                "editor.",
                "approval",
                "permission",
            ] {
                assert!(!command.starts_with(acting), "{command}");
            }
        }
    }

    #[test]
    fn a_branch_prefix_is_a_git_ref_part() {
        for good in ["feat", "fix", "team/feat", "v1.2", "a_b-c"] {
            assert!(branch_prefix_is_ref_part(good), "{good}");
        }
        for bad in [
            "",
            "-rf",
            "--upload-pack=x",
            "a b",
            "a..b",
            "a/",
            ".hidden",
            "a/.b",
            "x.lock",
            "a/b.lock",
            "a~b",
            "a^b",
            "a:b",
            "a?b",
            "a*b",
            "a[b",
            "a\\b",
            "a@{b",
            "a;rm",
            "a$(x)",
            "a\nb",
            "é",
            "a//b",
            "end.",
        ] {
            assert!(!branch_prefix_is_ref_part(bad), "{bad:?}");
        }
        assert!(!branch_prefix_is_ref_part(&"a".repeat(101)));
    }
}
