//! The independent oracle (Preview, slice 4m): an answer key written by an engine that did not build the change, and a
//! mutation gate that says whether its checks can catch a break at all.
//!
//! This part is pure: no store, no process, no file and no engine. The facts come in and a verdict goes out. The gate's
//! mutants come from a [`gate::MutationGate`], and only a fake exists, so nothing here runs a mutation tool or calls a
//! model; Crew never installs a mutation tool either. A cost in the oracle is a count of tokens or mutants, never a
//! dollar figure.
//!
//! The design's values (the gate on, two rounds, tasks that change code) are Proposed: Engineering sets the real ones.

mod gate;
mod key;
mod settings;
mod tools;

#[cfg(any(test, feature = "test-support"))]
pub use gate::ScriptedGate;
pub use gate::{
    AfterFailure, Check, Gate, GateStop, Mutant, MutantRun, MutationGate, NOT_VALIDATED_LABEL,
    Verdict, after_failure, label, survivor_is_covered,
};
pub use key::{Engine, Given, Independence, asks_person, independence};
pub use settings::{ROUNDS, Settings, proposed, wants_key};
pub use tools::{Language, NoGate, Tool, ready, tool_for};
