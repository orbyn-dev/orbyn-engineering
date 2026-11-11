//! What a peer may make this device do, and nothing more
//! ([No escalation across devices](../../../../docs/account.md#no-escalation-across-devices),
//! [threat model 5b](../../../../docs/account-threat-model.md)). Every rule here only
//! narrows: a value that fails is kept and passed on, never applied.

use orbyn_contracts::KeybindingRule;
use serde_json::{Map, Value};

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

/// The option ids each engine adapter may be given by a synced `default_model_options`.
///
/// **None.** The rule ([docs/account.md](../../../../docs/account.md#what-never-syncs)):
/// a setting that raises cost or widens what an agent may do never syncs. Every option an
/// adapter offers does one of those. Claude's `fast_mode` and Codex's `service_tier`
/// (`priority` and the like) buy faster, costlier processing; anything that becomes a
/// sandbox, approval or skip-permissions flag widens reach; ACP agents name their own
/// options and their `mode` is a permission mode. The lists stay so that a later change
/// has one place to add an id that provably lowers cost or changes neither.
const CLAUDE_OPTIONS: &[&str] = &[];
const CODEX_OPTIONS: &[&str] = &[];

/// Which adapter an engine id runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Adapter {
    Claude,
    Codex,
    /// Every other engine, run over ACP.
    Acp,
}

impl Adapter {
    pub fn of(engine: &str) -> Self {
        match engine {
            "claude" => Self::Claude,
            "codex" => Self::Codex,
            _ => Self::Acp,
        }
    }

    fn allowed(self) -> &'static [&'static str] {
        match self {
            Self::Claude => CLAUDE_OPTIONS,
            Self::Codex => CODEX_OPTIONS,
            Self::Acp => &[],
        }
    }
}

/// The options a synced `default_model_options` keeps for the engine. With no engine
/// named the default may be for either built-in adapter, so both lists apply. An ACP
/// engine keeps nothing.
pub fn filter_model_options(
    options: &Map<String, Value>,
    engine: Option<&str>,
) -> Map<String, Value> {
    let keep = |id: &str| match engine.filter(|engine| !engine.is_empty()) {
        Some(engine) => Adapter::of(engine).allowed().contains(&id),
        None => CLAUDE_OPTIONS.contains(&id) || CODEX_OPTIONS.contains(&id),
    };
    options
        .iter()
        .filter(|(id, _)| keep(id))
        .map(|(id, value)| (id.clone(), value.clone()))
        .collect()
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
    use serde_json::json;

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

    fn options(pairs: &[(&str, &str)]) -> Map<String, Value> {
        pairs
            .iter()
            .map(|(id, value)| ((*id).to_owned(), json!(value)))
            .collect()
    }

    const ADAPTERS: [(&str, &[(&str, &str)]); 4] = [
        (
            "claude",
            &[
                ("fast_mode", "on"),
                ("dangerously_skip_permissions", "true"),
                ("skip_permissions", "on"),
                ("permission_mode", "bypassPermissions"),
            ],
        ),
        (
            "codex",
            &[
                ("service_tier", "priority"),
                ("sandbox", "danger-full-access"),
                ("sandbox_mode", "danger-full-access"),
                ("approval_policy", "never"),
            ],
        ),
        (
            "an-acp-agent",
            &[
                ("mode", "bypass"),
                ("config:approval", "never"),
                ("config:sandbox", "off"),
                ("config:thought_level", "high"),
            ],
        ),
        (
            "",
            &[
                ("fast_mode", "on"),
                ("service_tier", "priority"),
                ("mode", "bypass"),
            ],
        ),
    ];

    #[test]
    fn no_adapter_keeps_any_option_cost_and_reach_never_sync() {
        for (engine, pairs) in ADAPTERS {
            let sent = options(pairs);
            let kept = filter_model_options(&sent, Some(engine));
            assert!(kept.is_empty(), "{engine}: {kept:?}");
        }
        let everything: Vec<(&str, &str)> = ADAPTERS
            .iter()
            .flat_map(|(_, p)| p.iter().copied())
            .collect();
        for engine in [None, Some("claude"), Some("codex"), Some("x")] {
            assert!(filter_model_options(&options(&everything), engine).is_empty());
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
