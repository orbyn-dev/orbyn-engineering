//! Who writes the answer key, and how independent that key is (slice 4m).
//!
//! A check written by the engine that built the change can pass because both made the same mistake. The key is written
//! by another engine, and another vendor where one is signed in. The author never sees the builder's code, notes,
//! messages or worktree, and the builder is never given the key (that second half is `crew::guard`'s leak check, which
//! every task already runs). With no other engine signed in, Crew asks the person.

/// An engine, as the key's author or the task's builder: the registry id, the vendor family and the model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Engine {
    /// The registry id (`claude`, `codex`, `opencode`).
    pub id: String,
    /// The engine this one is a setup of, or itself: two engines of one family are one vendor, as
    /// `crew::EngineView::family` has it. The independence rule reads this, not the id alone.
    pub family: String,
    /// The model a screen names beside the engine (`Sonnet 5.5`).
    pub model: String,
}

impl Engine {
    /// An engine the oracle knows: an id, the family it belongs to and the model it writes with.
    pub fn new(id: &str, family: &str, model: &str) -> Self {
        Self {
            id: id.to_owned(),
            family: family.to_owned(),
            model: model.to_owned(),
        }
    }
}

/// How the key's author stands against the change's builder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Independence {
    /// Another engine on another vendor: what Crew looks for, and no person is asked.
    AnotherVendor,
    /// Another engine of the builder's own vendor family. Independent of the build, not of the vendor: it is used, and
    /// the screen says so.
    SameVendor,
    /// The builder's own engine. Not independent: the key is used only when a person chooses it, and Crew asks each
    /// time ("Same engine as builder: not independent", "Crew asks each time").
    SameEngine,
    /// No engine other than the builder's is signed in, so Crew cannot choose one. The person is asked.
    NoSecondEngine,
}

/// How `author` stands against `builder`. `second_signed_in` is whether any engine other than the builder is signed in
/// on this device; when none is, the person is asked whatever engine the author names.
pub fn independence(author: &Engine, builder: &Engine, second_signed_in: bool) -> Independence {
    if !second_signed_in {
        return Independence::NoSecondEngine;
    }
    if author.id == builder.id {
        return Independence::SameEngine;
    }
    if author.family == builder.family {
        return Independence::SameVendor;
    }
    Independence::AnotherVendor
}

/// Whether the person is asked before such a key is used. A same-engine key and a device with no second engine always
/// wait for the person: no timer, agent or default answers instead.
pub fn asks_person(how: Independence) -> bool {
    matches!(how, Independence::SameEngine | Independence::NoSecondEngine)
}

/// What the author of the key was given to write it from.
///
/// The code, the builder's notes and messages, and its worktree are never part of it: a key written from them is the
/// builder's own answer again. A strengthening round also gets the survivors of a failed gate and the lines each one
/// changed, and still not the code as a whole.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Given {
    /// The brief: the task's words, scope and contract facts.
    pub brief: bool,
    /// The contract facts the task must hold to.
    pub contract_facts: bool,
    /// The builder's code. Never: the key would not be blind.
    pub code: bool,
    /// The builder's notes and messages. Never: they carry its own reasoning.
    pub builder_notes: bool,
    /// The builder's worktree. Never: it is the code under another name.
    pub worktree: bool,
    /// The survivors of a failed gate and the lines each one changed. Only a strengthening round has them.
    pub survivor_lines: bool,
}

impl Given {
    /// The first pass: the brief and the contract facts, and nothing else ("written before the builder finished").
    pub fn first_pass() -> Self {
        Self {
            brief: true,
            contract_facts: true,
            ..Self::default()
        }
    }

    /// A strengthening round: the same as the first pass, plus the survivors and the lines each one changed.
    pub fn strengthening() -> Self {
        Self {
            survivor_lines: true,
            ..Self::first_pass()
        }
    }

    /// False when the author was given anything of the builder's own: its code, its notes and messages, or its
    /// worktree. The survivors of a failed gate are not the build.
    pub fn blind_to_the_build(&self) -> bool {
        !(self.code || self.builder_notes || self.worktree)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claude() -> Engine {
        Engine::new("claude", "claude", "Sonnet 5.5")
    }

    fn codex() -> Engine {
        Engine::new("codex", "codex", "GPT-5.5")
    }

    #[test]
    fn another_engine_on_another_vendor_is_independent() {
        assert_eq!(
            independence(&claude(), &codex(), true),
            Independence::AnotherVendor
        );
    }

    #[test]
    fn another_engine_of_the_builders_family_is_the_same_vendor() {
        // A second setup of one engine is one vendor: only the family decides that, never the id.
        let other_codex = Engine::new("codex-work", "codex", "GPT-5.5");
        assert_eq!(
            independence(&other_codex, &codex(), true),
            Independence::SameVendor
        );
    }

    #[test]
    fn the_builders_own_engine_is_not_independent() {
        assert_eq!(
            independence(&codex(), &codex(), true),
            Independence::SameEngine
        );
        // The same id is the same engine even when the models differ: it is the engine that made the mistake.
        let other_model = Engine::new("codex", "codex", "GPT-5.1");
        assert_eq!(
            independence(&other_model, &codex(), true),
            Independence::SameEngine
        );
    }

    #[test]
    fn with_no_second_engine_the_person_is_asked() {
        // Nothing else is signed in, so even a different engine in hand means Crew asks rather than assumes.
        assert_eq!(
            independence(&claude(), &codex(), false),
            Independence::NoSecondEngine
        );
        assert_eq!(
            independence(&codex(), &codex(), false),
            Independence::NoSecondEngine
        );
    }

    #[test]
    fn the_person_is_asked_for_a_same_engine_key_and_with_no_second_engine() {
        assert!(asks_person(Independence::SameEngine));
        assert!(asks_person(Independence::NoSecondEngine));
        assert!(!asks_person(Independence::AnotherVendor));
        assert!(!asks_person(Independence::SameVendor));
    }

    #[test]
    fn the_first_pass_is_given_the_brief_and_the_contract_facts_only() {
        let given = Given::first_pass();
        assert!(given.brief);
        assert!(given.contract_facts);
        assert!(!given.code);
        assert!(!given.builder_notes);
        assert!(!given.worktree);
        assert!(!given.survivor_lines);
        assert!(given.blind_to_the_build());
    }

    #[test]
    fn a_key_written_from_the_code_is_not_blind() {
        let given = Given {
            code: true,
            ..Given::first_pass()
        };
        assert!(!given.blind_to_the_build());
    }

    #[test]
    fn the_builders_notes_or_worktree_make_the_key_not_blind() {
        for given in [
            Given {
                builder_notes: true,
                ..Given::first_pass()
            },
            Given {
                worktree: true,
                ..Given::first_pass()
            },
        ] {
            assert!(!given.blind_to_the_build(), "{given:?}");
        }
    }

    #[test]
    fn a_strengthening_round_gets_the_survivors_and_stays_blind_to_the_build() {
        let given = Given::strengthening();
        assert!(given.brief);
        assert!(given.contract_facts);
        assert!(given.survivor_lines);
        assert!(!given.code);
        assert!(!given.builder_notes);
        assert!(!given.worktree);
        assert!(given.blind_to_the_build());
    }
}
