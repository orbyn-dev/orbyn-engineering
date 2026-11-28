//! The mutation tool of a project's language, and whether the project has it (slice 4m).
//!
//! Crew runs a tool that is installed in the project and never installs one for you. A language with no tool, or a
//! tool that is not installed, makes the gate Not measured: never a pass, never a zero, and the key stays labelled not
//! validated until a person installs the tool and the gate runs.

/// A project language the gate can name. `Other` is every language Crew has no tool for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Rust,
    TypeScript,
    Python,
    /// Any other language: no tool Crew supports yet, so the gate makes no mutant.
    Other,
}

/// Why the gate makes no mutant. None of these is a pass, and none is a zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoGate {
    /// No tool Crew supports for the language ("No tool Crew supports yet").
    Unsupported,
    /// The language has a tool and the project does not have it installed. Crew never installs one.
    NotInstalled,
    /// The tool ran and made no mutant, so there is nothing to run the key against.
    NoMutant,
}

/// The tool the gate will run, and the version the project reports for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tool {
    /// The tool's own name (`Stryker`, `cargo-mutants`).
    pub name: &'static str,
    /// The version the project has, as its own check reported it.
    pub version: String,
}

/// The mutation tool of a language, or `None` for a language Crew has no tool for.
pub fn tool_for(language: Language) -> Option<&'static str> {
    match language {
        Language::Rust => Some("cargo-mutants"),
        Language::TypeScript => Some("Stryker"),
        Language::Python => Some("mutmut"),
        Language::Other => None,
    }
}

/// The tool the gate will run, or why it will make no mutant. `installed` is the version the project's own check found,
/// `None` when it is not installed there; nothing here installs it.
pub fn ready(language: Language, installed: Option<&str>) -> Result<Tool, NoGate> {
    match (tool_for(language), installed) {
        (None, _) => Err(NoGate::Unsupported),
        (Some(_), None) => Err(NoGate::NotInstalled),
        (Some(name), Some(version)) => Ok(Tool {
            name,
            version: version.to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_language_has_its_own_tool() {
        assert_eq!(tool_for(Language::Rust), Some("cargo-mutants"));
        assert_eq!(tool_for(Language::TypeScript), Some("Stryker"));
        assert_eq!(tool_for(Language::Python), Some("mutmut"));
    }

    #[test]
    fn any_other_language_has_no_tool_and_is_not_measured() {
        assert_eq!(tool_for(Language::Other), None);
        assert_eq!(
            ready(Language::Other, Some("1.0")),
            Err(NoGate::Unsupported)
        );
        assert_eq!(ready(Language::Other, None), Err(NoGate::Unsupported));
    }

    #[test]
    fn a_tool_the_project_does_not_have_is_not_measured() {
        assert_eq!(ready(Language::Rust, None), Err(NoGate::NotInstalled));
        assert_eq!(ready(Language::TypeScript, None), Err(NoGate::NotInstalled));
    }

    #[test]
    fn an_installed_tool_runs_with_its_own_version() {
        assert_eq!(
            ready(Language::TypeScript, Some("8.6")),
            Ok(Tool {
                name: "Stryker",
                version: "8.6".to_owned()
            })
        );
    }

    #[test]
    fn a_language_with_no_tool_never_reads_as_installed() {
        // The gate cannot run on a version it does not have a tool for: the answer is the language, never the version.
        assert_eq!(
            ready(Language::Other, Some("9.9")),
            Err(NoGate::Unsupported)
        );
    }
}
