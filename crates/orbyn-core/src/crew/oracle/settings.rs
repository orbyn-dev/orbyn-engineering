//! The independent oracle's settings (slice 4m): the mutation gate, the rounds a failed key is strengthened for, and
//! which tasks get a key at all.
//!
//! Every value here is the Designer's **Proposed** (Crew Designs v22); Engineering sets the real ones. Nothing in this
//! slice reads a config, a store or a file, and a screen that shows these values must keep saying Proposed.

/// Rounds a failing key goes back to its author for, before Crew asks the person. Proposed.
pub const ROUNDS: u8 = 2;

/// The oracle's settings: whether the gate runs, and how many rounds a failing key gets. Proposed in full.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Whether the mutation gate runs at all. Off means no key is ever validated: every one reads Not measured, never
    /// passed.
    pub gate: bool,
    /// Rounds a failed key is strengthened for before Crew asks the person (`gate::after_failure` takes it).
    pub rounds: u8,
}

/// The Designer's Proposed values: the gate is On, and a failed key gets two rounds before the person is asked.
pub fn proposed() -> Settings {
    Settings {
        gate: true,
        rounds: ROUNDS,
    }
}

/// Whether a task gets an independent answer key. Proposed: a task that changes code and has a check to write. A
/// documentation-only task has no mutants, so it is skipped and no key is written for it.
pub fn wants_key(changes_code: bool, has_check: bool) -> bool {
    changes_code && has_check
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_proposed_settings_are_the_designs() {
        let settings = proposed();
        assert!(settings.gate, "the gate is On");
        assert_eq!(settings.rounds, ROUNDS);
        assert_eq!(settings.rounds, 2, "two rounds, then the person");
    }

    #[test]
    fn a_code_task_with_a_check_to_write_gets_a_key() {
        assert!(wants_key(true, true));
    }

    #[test]
    fn a_documentation_only_task_gets_no_key() {
        // No mutants and no check to write: the task is skipped, not gated.
        assert!(!wants_key(false, true));
        assert!(!wants_key(false, false));
    }

    #[test]
    fn a_code_task_with_no_check_to_write_gets_no_key() {
        assert!(!wants_key(true, false));
    }
}
