//! The mutation gate (slice 4m): Crew makes small deliberate breaks in the changed code and runs the answer key's
//! checks on each.
//!
//! A key that kills no mutant is not an oracle: it passes on broken code, so a pass would prove nothing and the task
//! does not pass on it. A key that kills at least one mutant can catch a break, which is all a passed gate claims: not
//! that the change is right, and not that the survivors are harmless. Crew never decides which survivors matter; it
//! lists them as the brief for a stronger check.
//!
//! Only a fake runs here (`ScriptedGate`): no mutation tool starts, no engine is called and no file is written. The
//! mutants are the tool's, and a mutant's "what changed" is one line from it, never the file's text.

use super::tools::{Language, NoGate};

/// A count of `usize` as the `u32` a verdict keeps. A count above `u32::MAX` shows as `u32::MAX`: it never wraps to a
/// small number, which would read as a better key than it is.
fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// One deliberate break the tool made in the changed code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mutant {
    /// The tool's own id (`m7`).
    pub id: String,
    /// The file it is in, as the repository spells it.
    pub file: String,
    /// The line it is on.
    pub line: u32,
    /// What the tool changed, one line (`count > limit -> count >= limit`). Never the file's text.
    pub change: String,
}

/// One check of the answer key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    /// What names the check in a list (`Answer key check 1`).
    pub id: String,
    /// The check's own words, one line.
    pub text: String,
    /// The file the check is written in.
    pub file: String,
}

/// What one mutant's run showed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MutantRun {
    pub mutant: Mutant,
    /// The checks that killed this mutant, in the order they were run, and empty when it survived. One mutant can be
    /// killed by more than one check, which is why the per-check kills can add up to more than the killed total.
    pub killed_by: Vec<String>,
}

impl MutantRun {
    /// Whether the key caught this break. A survivor is a break no check caught.
    pub fn killed(&self) -> bool {
        !self.killed_by.is_empty()
    }
}

/// What the gate says. A passed gate is the only one that labels a key validated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Nothing was measured: no mutant was made, so there is no result. Not a pass and not a zero.
    NotMeasured,
    /// Still running: `run` of `made` mutants have been run so far, and the tool will finish by itself.
    Running {
        run: u32,
        made: u32,
        killed: u32,
        survived: u32,
    },
    /// At least one mutant killed: the checks can catch a break. Says nothing about the change being right, and
    /// nothing about whether the survivors are harmless.
    Passed {
        made: u32,
        killed: u32,
        survived: u32,
    },
    /// Every mutant survived: the checks pass on broken code, so the key is not an oracle and the task does not pass
    /// on it.
    Failed { made: u32 },
}

/// Why a gate has no result at all. A stale or partial kill count is never reported as one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GateStop {
    /// Nothing was measured: no tool for the language, or the tool is not installed in the project.
    NotMeasured(NoGate),
    /// The tool stopped part-way, so there is no result: `ran` of `made` mutants ran before it stopped, and nothing it
    /// did is counted. `reason` is the tool's own, one line, and holds no file text.
    Stopped { ran: u32, made: u32, reason: String },
}

/// A gate's run: the mutants the tool made and what each one's run showed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gate {
    /// The tool's own count of mutants it made.
    made: u32,
    /// The mutants that have run, in the tool's order. A gate still running has fewer of these than `made`.
    runs: Vec<MutantRun>,
}

impl Gate {
    /// A run of `made` mutants, of which `runs` have finished. `made` is never reported below the number of runs: a
    /// tool that says it made 8 and hands back 11 runs reads as 11, never as a share above the whole.
    pub fn new(made: u32, runs: Vec<MutantRun>) -> Self {
        Self {
            made: made.max(count(runs.len())),
            runs,
        }
    }

    /// How many mutants the tool said it made.
    pub fn made(&self) -> u32 {
        self.made
    }

    /// How many have run so far.
    pub fn run(&self) -> u32 {
        count(self.runs.len())
    }

    /// How many the key caught.
    pub fn killed(&self) -> u32 {
        count(self.runs.iter().filter(|run| run.killed()).count())
    }

    /// How many got through every check.
    pub fn survived(&self) -> u32 {
        self.run().saturating_sub(self.killed())
    }

    /// The breaks no check caught. A survivor is a break every check missed; which of them matter is not Crew's to
    /// decide, so they are listed and the author may write a check for any the person chooses.
    pub fn survivors(&self) -> Vec<&MutantRun> {
        self.runs.iter().filter(|run| !run.killed()).collect()
    }

    /// Each named check's own kills, in the order the checks were named. A check that kills no mutant is listed with 0:
    /// "kills no mutant on its own" is a fact a person reads, never an omission.
    pub fn kills_by_check(&self, checks: &[Check]) -> Vec<(String, u32)> {
        checks
            .iter()
            .map(|check| {
                let kills = self
                    .runs
                    .iter()
                    .filter(|run| run.killed_by.iter().any(|id| id == &check.id))
                    .count();
                (check.id.clone(), count(kills))
            })
            .collect()
    }

    /// The gate's verdict. `NotMeasured` when no mutant was made at all, `Running` until the last one has run.
    pub fn verdict(&self) -> Verdict {
        if self.made == 0 {
            return Verdict::NotMeasured;
        }
        let run = self.run();
        let killed = self.killed();
        let survived = self.survived();
        if run < self.made {
            return Verdict::Running {
                run,
                made: self.made,
                killed,
                survived,
            };
        }
        if killed == 0 {
            Verdict::Failed { made: self.made }
        } else {
            Verdict::Passed {
                made: self.made,
                killed,
                survived,
            }
        }
    }
}

/// How a task on this key is labelled wherever it is shown. Only a passed gate removes the label.
pub const NOT_VALIDATED_LABEL: &str = "Answer key not validated";

/// The label the task carries, from what the gate said: a passed gate is the only verdict that removes it, so a key
/// with no tool, a gate still running and a failed gate a person chose to go on past all read not validated.
pub fn label(verdict: Verdict) -> Option<&'static str> {
    match verdict {
        Verdict::Passed { .. } => None,
        _ => Some(NOT_VALIDATED_LABEL),
    }
}

/// What a failed gate does next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AfterFailure {
    /// The key goes back to its author with the survivors, for round `round` of `rounds`.
    Strengthen {
        round: u8,
        rounds: u8,
        survivors: u32,
    },
    /// The last round failed too: Crew asks the person and goes no further without their say.
    NeedsPerson { rounds: u8 },
}

/// The rounds a key has already failed, the design's `rounds`, and the survivors decide what happens next. Nothing is
/// merged while the key is not an oracle unless the person chooses to go on without the gate.
pub fn after_failure(failed_rounds: u8, rounds: u8, survivors: u32) -> AfterFailure {
    if failed_rounds < rounds {
        AfterFailure::Strengthen {
            round: failed_rounds.saturating_add(1),
            rounds,
            survivors,
        }
    } else {
        AfterFailure::NeedsPerson { rounds }
    }
}

/// A new check is proved the same way the key was: it must fail on the survivor and pass on the real code. Either half
/// alone proves nothing, so a check that fails on both, or passes on both, is not a check for that survivor.
pub fn survivor_is_covered(fails_on_the_survivor: bool, passes_on_the_real_code: bool) -> bool {
    fails_on_the_survivor && passes_on_the_real_code
}

/// What runs the key against the mutants.
///
/// Only a fake exists in this slice, so nothing starts a tool and no engine is called. `Ok` always carries at least one
/// mutant: a run that made none is a stop, never an empty pass.
pub trait MutationGate {
    /// Makes the mutants of `language`'s changed code and runs `checks` on each. `Err` is no result at all: the key is
    /// Not measured, or the tool stopped part-way and nothing it did is counted.
    fn run(&self, language: Language, checks: &[Check]) -> std::result::Result<Gate, GateStop>;
}

/// A gate whose mutants and kills are told to it, for tests and for everything this slice has: no tool runs.
#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Debug, Default)]
pub struct ScriptedGate {
    made: u32,
    /// Per mutant index, the checks that kill it. A mutant past the list survives.
    killed_by: Vec<Vec<String>>,
    /// Why the run stops, when it does not finish.
    stop: Option<ScriptedStop>,
}

#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Debug)]
enum ScriptedStop {
    NotMeasured(NoGate),
    After { ran: u32 },
}

#[cfg(any(test, feature = "test-support"))]
impl ScriptedGate {
    /// `made` mutants, none of which the key kills: the gate fails.
    pub fn surviving(made: u32) -> Self {
        Self {
            made,
            ..Self::default()
        }
    }

    /// `made` mutants, where `killed_by[i]` names the checks that kill mutant `i + 1`. A mutant past the list survives.
    pub fn killing(made: u32, killed_by: Vec<Vec<&str>>) -> Self {
        Self {
            made,
            killed_by: killed_by
                .into_iter()
                .map(|checks| checks.into_iter().map(str::to_owned).collect())
                .collect(),
            stop: None,
        }
    }

    /// The tool stops after `ran` mutants, as a tool that exits part-way does: there is no result.
    pub fn stopping(made: u32, ran: u32) -> Self {
        Self {
            made,
            stop: Some(ScriptedStop::After { ran }),
            ..Self::default()
        }
    }

    /// No tool for the language, so nothing is measured.
    pub fn unsupported() -> Self {
        Self {
            stop: Some(ScriptedStop::NotMeasured(NoGate::Unsupported)),
            ..Self::default()
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
fn scripted_mutant(n: u32) -> Mutant {
    Mutant {
        id: format!("m{}", n.saturating_add(1)),
        file: "src/changed.rs".to_owned(),
        line: n.saturating_add(1),
        change: "one small break".to_owned(),
    }
}

#[cfg(any(test, feature = "test-support"))]
impl MutationGate for ScriptedGate {
    fn run(&self, _language: Language, checks: &[Check]) -> std::result::Result<Gate, GateStop> {
        match &self.stop {
            Some(ScriptedStop::NotMeasured(why)) => return Err(GateStop::NotMeasured(*why)),
            Some(ScriptedStop::After { ran }) => {
                return Err(GateStop::Stopped {
                    ran: *ran,
                    made: self.made,
                    reason: "the tool stopped before the last mutant".to_owned(),
                });
            }
            None => {}
        }
        let runs = (0..self.made)
            .map(|index| {
                let killed_by = self
                    .killed_by
                    .get(usize::try_from(index).unwrap_or(usize::MAX))
                    .map(Vec::as_slice)
                    .unwrap_or_default()
                    .iter()
                    // The tool runs this key's checks, so a kill it names for a check the key does not hold is no kill
                    // by this key.
                    .filter(|id| checks.iter().any(|check| &check.id == *id))
                    .cloned()
                    .collect();
                MutantRun {
                    mutant: scripted_mutant(index),
                    killed_by,
                }
            })
            .collect();
        Ok(Gate::new(self.made, runs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(id: &str) -> Check {
        Check {
            id: id.to_owned(),
            text: format!("the check called {id}"),
            file: "tests/key.rs".to_owned(),
        }
    }

    fn mutant(n: u32) -> Mutant {
        Mutant {
            id: format!("m{n}"),
            file: "auth/rate-limit.ts".to_owned(),
            line: 40 + n,
            change: "count > limit -> count >= limit".to_owned(),
        }
    }

    fn ran(n: u32, killers: &[&str]) -> MutantRun {
        MutantRun {
            mutant: mutant(n),
            killed_by: killers.iter().map(|id| (*id).to_owned()).collect(),
        }
    }

    #[test]
    fn one_killed_mutant_passes_the_gate() {
        let gate = Gate::new(3, vec![ran(1, &["c1"]), ran(2, &[]), ran(3, &[])]);
        assert_eq!(
            gate.verdict(),
            Verdict::Passed {
                made: 3,
                killed: 1,
                survived: 2
            }
        );
    }

    #[test]
    fn every_mutant_surviving_fails_the_gate() {
        let gate = Gate::new(18, (1..=18).map(|n| ran(n, &[])).collect());
        assert_eq!(gate.verdict(), Verdict::Failed { made: 18 });
    }

    #[test]
    fn the_verdict_counts_mutants_made_killed_and_survived() {
        let gate = Gate::new(
            18,
            vec![
                ran(1, &["c1"]),
                ran(2, &["c2"]),
                ran(3, &["c1", "c2"]),
                ran(4, &[]),
            ],
        );
        assert_eq!(gate.made(), 18);
        assert_eq!(gate.run(), 4);
        assert_eq!(gate.killed(), 3);
        assert_eq!(gate.survived(), 1);
    }

    #[test]
    fn a_gate_still_running_says_how_many_have_run() {
        // 11 of 18 have run: 9 killed and 2 survived, and the rest have not run yet.
        let runs = (1..=11)
            .map(|n| if n <= 9 { ran(n, &["c1"]) } else { ran(n, &[]) })
            .collect();
        let gate = Gate::new(18, runs);
        assert_eq!(
            gate.verdict(),
            Verdict::Running {
                run: 11,
                made: 18,
                killed: 9,
                survived: 2
            }
        );
    }

    #[test]
    fn no_mutant_at_all_is_not_measured_and_never_a_pass() {
        let gate = Gate::new(0, Vec::new());
        assert_eq!(gate.verdict(), Verdict::NotMeasured);
    }

    #[test]
    fn a_reported_count_below_the_runs_is_raised_to_the_runs() {
        // A tool that says it made 2 and hands back 3 runs is not a share above the whole: it made 3.
        let gate = Gate::new(2, vec![ran(1, &[]), ran(2, &[]), ran(3, &["c1"])]);
        assert_eq!(gate.made(), 3);
        assert_eq!(gate.run(), 3);
        assert_eq!(
            gate.verdict(),
            Verdict::Passed {
                made: 3,
                killed: 1,
                survived: 2
            }
        );
    }

    #[test]
    fn a_survivor_is_listed_with_where_it_is_and_what_changed() {
        let gate = Gate::new(2, vec![ran(1, &["c1"]), ran(2, &[])]);
        let survivors = gate.survivors();
        assert_eq!(survivors.len(), 1);
        assert_eq!(survivors[0].mutant.id, "m2");
        assert_eq!(survivors[0].mutant.file, "auth/rate-limit.ts");
        assert_eq!(survivors[0].mutant.line, 42);
        assert_eq!(
            survivors[0].mutant.change,
            "count > limit -> count >= limit"
        );
    }

    #[test]
    fn each_check_shows_its_own_kills_and_a_check_that_kills_none_is_listed() {
        // The frame's own shape: 3 checks, 2 of 3 killed and 1 that kills no mutant on its own.
        let checks = vec![check("c1"), check("c2"), check("c3")];
        let gate = Gate::new(3, vec![ran(1, &["c1"]), ran(2, &["c1", "c2"]), ran(3, &[])]);
        assert_eq!(
            gate.kills_by_check(&checks),
            vec![
                ("c1".to_owned(), 2),
                ("c2".to_owned(), 1),
                ("c3".to_owned(), 0)
            ]
        );
    }

    #[test]
    fn a_mutant_killed_by_two_checks_counts_for_both() {
        // Two checks kill one mutant, so the per-check kills (2) pass the killed total (1): the frames' counts do that.
        let checks = vec![check("c1"), check("c2")];
        let gate = Gate::new(1, vec![ran(1, &["c1", "c2"])]);
        assert_eq!(gate.killed(), 1);
        assert_eq!(
            gate.kills_by_check(&checks),
            vec![("c1".to_owned(), 1), ("c2".to_owned(), 1)]
        );
    }

    #[test]
    fn only_a_passed_gate_removes_the_not_validated_label() {
        assert_eq!(
            label(Verdict::Passed {
                made: 18,
                killed: 15,
                survived: 3
            }),
            None
        );
        assert_eq!(
            label(Verdict::Failed { made: 18 }),
            Some(NOT_VALIDATED_LABEL)
        );
        assert_eq!(label(Verdict::NotMeasured), Some(NOT_VALIDATED_LABEL));
        assert_eq!(
            label(Verdict::Running {
                run: 4,
                made: 18,
                killed: 3,
                survived: 1
            }),
            Some(NOT_VALIDATED_LABEL)
        );
        assert!(!NOT_VALIDATED_LABEL.contains('$'));
    }

    #[test]
    fn a_failed_gate_goes_back_to_its_author_for_the_first_round() {
        assert_eq!(
            after_failure(0, 2, 18),
            AfterFailure::Strengthen {
                round: 1,
                rounds: 2,
                survivors: 18
            }
        );
    }

    #[test]
    fn the_second_failure_is_the_last_round() {
        assert_eq!(
            after_failure(1, 2, 5),
            AfterFailure::Strengthen {
                round: 2,
                rounds: 2,
                survivors: 5
            }
        );
    }

    #[test]
    fn after_the_last_round_crew_asks_the_person() {
        assert_eq!(
            after_failure(2, 2, 0),
            AfterFailure::NeedsPerson { rounds: 2 }
        );
    }

    #[test]
    fn a_gate_with_no_rounds_asks_the_person_at_once() {
        assert_eq!(
            after_failure(0, 0, 18),
            AfterFailure::NeedsPerson { rounds: 0 }
        );
    }

    #[test]
    fn a_new_check_is_proved_only_when_it_fails_on_the_survivor_and_passes_on_the_real_code() {
        assert!(survivor_is_covered(true, true));
        assert!(
            !survivor_is_covered(true, false),
            "fails on both: proves nothing"
        );
        assert!(
            !survivor_is_covered(false, true),
            "passes on both: proves nothing"
        );
        assert!(!survivor_is_covered(false, false));
    }

    #[test]
    fn no_count_in_a_verdict_is_money() {
        // A cost in the oracle is a count of mutants or tokens, never a dollar figure, and never a price beside one.
        for verdict in [
            Verdict::Passed {
                made: 18,
                killed: 15,
                survived: 3,
            },
            Verdict::Failed { made: 18 },
            Verdict::Running {
                run: 11,
                made: 18,
                killed: 9,
                survived: 2,
            },
        ] {
            assert!(!format!("{verdict:?}").contains('$'), "{verdict:?}");
        }
    }

    #[test]
    fn the_scripted_gate_makes_the_mutants_it_was_told_to() {
        let gate = ScriptedGate::killing(3, vec![vec!["c1"], vec![], vec![]])
            .run(Language::TypeScript, &[check("c1")])
            .unwrap_or_else(|stop| panic!("the fake gate ran: {stop:?}"));
        assert_eq!(gate.made(), 3);
        assert_eq!(gate.run(), 3);
        assert_eq!(gate.killed(), 1);
        assert_eq!(
            gate.runs
                .iter()
                .map(|run| run.mutant.id.as_str())
                .collect::<Vec<_>>(),
            vec!["m1", "m2", "m3"]
        );
    }

    #[test]
    fn the_scripted_gate_never_blames_a_check_the_key_does_not_have() {
        // The tool runs this key's checks, so a kill it names that the key does not hold is no kill by this key.
        let gate = ScriptedGate::killing(1, vec![vec!["c9"]])
            .run(Language::TypeScript, &[check("c1")])
            .unwrap_or_else(|stop| panic!("the fake gate ran: {stop:?}"));
        assert_eq!(gate.killed(), 0);
        assert_eq!(gate.verdict(), Verdict::Failed { made: 1 });
    }

    #[test]
    fn a_tool_that_stops_part_way_has_no_result() {
        // There is no partial result to read: the stop carries what ran, and no kill count at all.
        match ScriptedGate::stopping(18, 4).run(Language::TypeScript, &[check("c1")]) {
            Err(GateStop::Stopped { ran, made, reason }) => {
                assert_eq!((ran, made), (4, 18));
                assert!(reason.contains("stopped"), "{reason}");
            }
            other => panic!("a stopped tool is no result, not {other:?}"),
        }
    }

    #[test]
    fn a_language_with_no_tool_is_not_measured_and_not_a_pass() {
        assert_eq!(
            ScriptedGate::unsupported().run(Language::Other, &[check("c1")]),
            Err(GateStop::NotMeasured(NoGate::Unsupported))
        );
    }
}
