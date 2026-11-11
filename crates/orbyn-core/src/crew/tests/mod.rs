//! Crew on fake engines. `filters`, `scoring` and `overrides` run the pure router and `stamp`
//! checks drafts and the guards: no process starts. `lifecycle` and `verify` test the task
//! rules and the verifier. `core`, `starts` and `runs` use a real `Core` with a scripted engine:
//! `starts` for what a start refuses, `runs` for a task from its first turn to its end.

mod core;
mod fake;
mod filters;
mod hardening;
mod harness;
mod lifecycle;
mod overrides;
mod protect;
mod runs;
mod scoring;
mod stamp;
mod starts;
mod verify;
