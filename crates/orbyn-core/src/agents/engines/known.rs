//! Engines Orbyn can run with nothing set up, because each speaks ACP from its own
//! CLI. One is offered only when its program is on this machine, and its own login
//! stays its own: an agent that is not signed in says so when it runs.

use orbyn_contracts::{CustomEngine, EngineTier};

use super::instances::LOCAL_ACP_DRIVER;
use crate::error::{CoreError, Result};

/// One CLI and how to reach its ACP mode.
pub struct KnownEngine {
    pub id: &'static str,
    pub name: &'static str,
    /// Tried in order. The first found is what runs.
    pub programs: &'static [&'static str],
    pub args: &'static [&'static str],
    /// Set for this process alone, to keep the agent asking before it acts.
    pub env: &'static [(&'static str, &'static str)],
    /// False for an agent that cannot ask before an edit, which rule 8 must label.
    pub asks: bool,
    /// Packages that install a different CLI under the same program name. A program
    /// whose resolved file lies inside one of these is not this engine.
    pub not_from: &'static [&'static str],
}

pub const KNOWN_ENGINES: &[KnownEngine] = &[
    KnownEngine {
        id: "antigravity",
        name: "Antigravity",
        programs: &["agy_acp_server", "agy_acp_server.par"],
        args: &[],
        env: &[],
        asks: true,
        not_from: &[],
    },
    KnownEngine {
        id: "cline",
        name: "Cline",
        programs: &["cline"],
        args: &["--acp"],
        env: &[],
        asks: true,
        not_from: &[],
    },
    KnownEngine {
        id: "commandcode",
        name: "Command Code",
        programs: &["command-code"],
        args: &["acp"],
        env: &[],
        asks: true,
        not_from: &[],
    },
    KnownEngine {
        id: "copilot",
        name: "GitHub Copilot",
        programs: &["copilot"],
        args: &["--no-auto-update", "--acp", "--stdio"],
        env: &[],
        asks: true,
        not_from: &[],
    },
    KnownEngine {
        id: "cursor",
        name: "Cursor",
        programs: &["cursor-agent"],
        args: &["acp"],
        env: &[],
        asks: true,
        not_from: &[],
    },
    KnownEngine {
        id: "deepseek",
        name: "DeepSeek",
        programs: &["dsh"],
        args: &["--profile", "acp"],
        env: &[("DSH_PERMISSION_MODE", "workspace-write")],
        asks: false,
        not_from: &[],
    },
    KnownEngine {
        id: "devin",
        name: "Devin",
        programs: &["devin"],
        args: &["acp"],
        env: &[],
        asks: true,
        not_from: &[],
    },
    KnownEngine {
        id: "fx",
        name: "fx",
        programs: &["fx"],
        args: &["acp"],
        env: &[],
        // Its starting mode is unobserved, and its automatic mode writes without
        // asking, so it stays labelled until a run shows it asks.
        asks: false,
        // The `fx` JSON viewer from npm or Homebrew has the same name.
        not_from: &["node_modules/fx", "Cellar/fx"],
    },
    KnownEngine {
        id: "grok",
        name: "Grok Build",
        programs: &["grok"],
        // Global flags go before `agent`. Stating the mode outranks a saved `yolo`.
        args: &[
            "--no-auto-update",
            "--permission-mode",
            "default",
            "agent",
            "stdio",
        ],
        env: &[],
        asks: true,
        // An unrelated npm CLI, with no ACP mode, also installs itself as `grok`.
        not_from: &["@vibe-kit/grok-cli"],
    },
    KnownEngine {
        id: "hermes",
        name: "Hermes Agent",
        programs: &["hermes"],
        args: &["acp"],
        env: &[],
        asks: true,
        not_from: &[],
    },
    KnownEngine {
        id: "kilo",
        name: "Kilo Code",
        programs: &["kilo"],
        args: &["acp"],
        env: &[("KILO_DISABLE_AUTOUPDATE", "true")],
        asks: true,
        not_from: &[],
    },
    KnownEngine {
        id: "kimi",
        name: "Kimi Code",
        programs: &["kimi"],
        args: &["acp"],
        env: &[],
        asks: true,
        not_from: &[],
    },
    KnownEngine {
        id: "opencode",
        name: "OpenCode",
        programs: &["opencode"],
        args: &["acp"],
        // Its defaults allow edits. Inline settings outrank the person's own file.
        env: &[("OPENCODE_CONFIG_CONTENT", r#"{"permission":{"*":"ask"}}"#)],
        asks: true,
        not_from: &[],
    },
];

impl KnownEngine {
    /// The program to run: the first one found, or the first named so that a thread
    /// on an engine since removed fails with its proper name.
    fn program(&self) -> &'static str {
        self.programs
            .iter()
            .copied()
            .find(|program| self.found(program))
            .unwrap_or(self.programs[0])
    }

    pub fn is_installed(&self) -> bool {
        self.programs.iter().any(|program| self.found(program))
    }

    /// Found on this machine, and not another CLI that shares the name.
    fn found(&self, program: &str) -> bool {
        crate::host::process::resolve(program).is_some_and(|command| !self.is_other(&command))
    }

    fn is_other(&self, command: &crate::host::process::ResolvedCommand) -> bool {
        let mut files = vec![command.path.clone(), command.program.clone()];
        files.extend(command.prefix.iter().map(std::path::PathBuf::from));
        if let Ok(real) = std::fs::canonicalize(&command.path) {
            files.push(real);
        }
        files.iter().any(|file| inside_any(file, self.not_from))
    }

    /// Where a program of this engine's name was found that is another CLI, when no real
    /// one was: the file the person would otherwise expect Orbyn to start.
    pub fn refused(&self) -> Option<std::path::PathBuf> {
        if self.is_installed() {
            return None;
        }
        self.programs.iter().find_map(|program| {
            crate::host::process::resolve(program)
                .filter(|command| self.is_other(command))
                .map(|command| command.path)
        })
    }

    pub fn as_custom(&self) -> CustomEngine {
        CustomEngine {
            id: self.id.to_owned(),
            name: self.name.to_owned(),
            tier: EngineTier::Acp,
            target: self.program().to_owned(),
            args: self.args.iter().map(|arg| (*arg).to_owned()).collect(),
        }
    }
}

/// True when `file` has the components of one of `packages` (an npm scope and name
/// such as `@scope/name`) in a row, on either platform's separators.
fn inside_any(file: &std::path::Path, packages: &[&str]) -> bool {
    let parts: Vec<String> = file
        .components()
        .map(|part| part.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    packages.iter().any(|package| {
        let wanted: Vec<String> = package.split('/').map(str::to_lowercase).collect();
        parts
            .windows(wanted.len())
            .any(|window| window == wanted.as_slice())
    })
}

pub fn known_engine(id: &str) -> Option<&'static KnownEngine> {
    KNOWN_ENGINES.iter().find(|engine| engine.id == id)
}

/// A lowercase id from a name: runs of anything but letters and digits become one `-`.
pub fn slug(name: &str) -> String {
    let text: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    text.split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Fills in a missing id from the name, then refuses an engine that is incomplete,
/// uses a reserved id, claims a built-in tier or has an AG-UI target that is not a URL.
pub fn prepare_custom_engine(engine: &mut CustomEngine) -> Result<()> {
    if engine.id.is_empty() {
        engine.id = slug(&engine.name);
    }
    if engine.id.is_empty() || engine.name.trim().is_empty() || engine.target.trim().is_empty() {
        return Err(CoreError::Invalid(
            "Name the engine and give its command or URL.".into(),
        ));
    }
    if engine.id == LOCAL_ACP_DRIVER {
        return Err(CoreError::Invalid(
            "That engine id is reserved for local ACP instances.".into(),
        ));
    }
    if matches!(engine.id.as_str(), "claude" | "codex") || engine.tier == EngineTier::Native {
        return Err(CoreError::Invalid("That engine is built in.".into()));
    }
    if engine.tier == EngineTier::Agui
        && !(engine.target.starts_with("http://") || engine.target.starts_with("https://"))
    {
        return Err(CoreError::Invalid(
            "An AG-UI engine needs an http or https URL.".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_none_takes_a_built_in_name() {
        for engine in KNOWN_ENGINES {
            let same = KNOWN_ENGINES.iter().filter(|e| e.id == engine.id).count();
            assert_eq!(same, 1, "{}", engine.id);
            assert!(!matches!(engine.id, "claude" | "codex"));
            assert!(!engine.programs.is_empty());
        }
    }

    #[test]
    fn each_launch_is_only_what_the_table_says() {
        let copilot = known_engine("copilot").unwrap().as_custom();
        assert_eq!(copilot.tier, EngineTier::Acp);
        assert_eq!(copilot.args, ["--no-auto-update", "--acp", "--stdio"]);
        assert_eq!(known_engine("cursor").unwrap().programs, ["cursor-agent"]);
        assert!(known_engine("nothing").is_none());
    }

    #[test]
    fn a_same_named_cli_from_another_package_is_not_the_engine() {
        let other = std::path::Path::new("lib")
            .join("node_modules")
            .join("@vibe-kit")
            .join("grok-cli")
            .join("dist")
            .join("index.js");
        let grok = known_engine("grok").unwrap();
        assert!(inside_any(&other, grok.not_from));
        let real = std::path::Path::new("bin").join("grok");
        assert!(!inside_any(&real, grok.not_from));
        let near = std::path::Path::new("node_modules")
            .join("@vibe-kit")
            .join("grok-cli-fork");
        assert!(!inside_any(&near, grok.not_from));
        let unix =
            std::path::Path::new("/usr/local/lib/node_modules/@vibe-kit/grok-cli/dist/index.js");
        assert!(inside_any(unix, grok.not_from));
        let fx = known_engine("fx").unwrap();
        let viewer = std::path::Path::new("/opt/homebrew/Cellar/fx/39.1.0/bin/fx");
        assert!(inside_any(viewer, fx.not_from));
        let npm = std::path::Path::new("C:\\npm\\node_modules\\fx\\index.js");
        assert_eq!(inside_any(npm, fx.not_from), cfg!(windows));
        let agent = std::path::Path::new("/Users/me/.fx/bin/fx");
        assert!(!inside_any(agent, fx.not_from));
    }

    #[test]
    fn only_the_engines_that_may_not_ask_are_marked() {
        let silent: Vec<_> = KNOWN_ENGINES
            .iter()
            .filter(|e| !e.asks)
            .map(|e| e.id)
            .collect();
        assert_eq!(silent, ["deepseek", "fx"]);
    }
}

#[cfg(test)]
mod custom_engine_tests {
    use super::*;

    fn engine(id: &str, name: &str, tier: EngineTier, target: &str) -> CustomEngine {
        CustomEngine {
            id: id.into(),
            name: name.into(),
            tier,
            target: target.into(),
            args: Vec::new(),
        }
    }

    fn message(result: Result<()>) -> String {
        match result {
            Err(CoreError::Invalid(message)) => message,
            other => panic!("expected an invalid-input error, got {other:?}"),
        }
    }

    #[test]
    fn slug_joins_words_and_drops_edges() {
        assert_eq!(slug("My  Agent_2!"), "my-agent-2");
        assert_eq!(slug("--Already-slugged--"), "already-slugged");
        assert_eq!(slug("!!!"), "");
    }

    #[test]
    fn a_missing_id_comes_from_the_name() {
        let mut named = engine("", "Open Code", EngineTier::Acp, "opencode acp");
        prepare_custom_engine(&mut named).unwrap();
        assert_eq!(named.id, "open-code");
        let mut kept = engine("mine", "Open Code", EngineTier::Acp, "opencode acp");
        prepare_custom_engine(&mut kept).unwrap();
        assert_eq!(kept.id, "mine");
    }

    #[test]
    fn an_incomplete_engine_is_refused() {
        for (id, name, target) in [("", "!!!", "x"), ("a", "  ", "x"), ("a", "A", " ")] {
            let mut incomplete = engine(id, name, EngineTier::Acp, target);
            assert_eq!(
                message(prepare_custom_engine(&mut incomplete)),
                "Name the engine and give its command or URL."
            );
        }
    }

    #[test]
    fn reserved_and_built_in_ids_are_refused() {
        let mut reserved = engine(LOCAL_ACP_DRIVER, "A", EngineTier::Acp, "x");
        assert_eq!(
            message(prepare_custom_engine(&mut reserved)),
            "That engine id is reserved for local ACP instances."
        );
        for id in ["claude", "codex"] {
            let mut built_in = engine(id, "A", EngineTier::Acp, "x");
            assert_eq!(
                message(prepare_custom_engine(&mut built_in)),
                "That engine is built in."
            );
        }
        let mut native = engine("mine", "A", EngineTier::Native, "x");
        assert_eq!(
            message(prepare_custom_engine(&mut native)),
            "That engine is built in."
        );
    }

    #[test]
    fn an_agui_engine_needs_a_url() {
        let mut bad = engine("a", "A", EngineTier::Agui, "agent --serve");
        assert_eq!(
            message(prepare_custom_engine(&mut bad)),
            "An AG-UI engine needs an http or https URL."
        );
        for target in ["http://localhost:1", "https://example.test/agent"] {
            let mut good = engine("a", "A", EngineTier::Agui, target);
            prepare_custom_engine(&mut good).unwrap();
        }
    }
}
