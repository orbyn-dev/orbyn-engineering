//! Crew on fake engines. `filters`, `scoring` and `overrides` run the pure router and `stamp`
//! checks drafts and the guards: no process starts. `lifecycle` and `verify` test the task
//! rules and the verifier. `core`, `starts` and `runs` use a real `Core` with a scripted engine:
//! `starts` for what a start refuses, `runs` for a task from its first turn to its end.

mod autopilot;
mod autopilot_areas;
mod autopilot_enforce;
mod autopilot_guard;
mod autopilot_held;
mod autopilot_merge;
mod autopilot_risk;
mod autopilot_rules;
mod autopilot_support;
mod autopilot_watch;
mod background;
mod bus;
mod bus_flood;
mod bus_guards;
mod bus_review;
mod bus_support;
mod cancel;
mod classes;
mod confined;
mod core;
mod fake;
mod filters;
mod hardening;
mod harness;
mod lifecycle;
mod overrides;
mod protect;
mod provider_facts;
mod review;
mod review_fix;
mod review_rules;
mod review_support;
mod runs;
mod scoring;
mod stamp;
mod starts;
mod verify;
