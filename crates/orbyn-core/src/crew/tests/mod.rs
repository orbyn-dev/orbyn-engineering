//! Crew on fake engines. `filters`, `scoring` and `overrides` run the pure router and `stamp`
//! checks drafts and the guards: no process starts. `lifecycle` and `verify` test the task
//! rules and the verifier. `core`, `starts` and `runs` use a real `Core` with a scripted engine:
//! `starts` for what a start refuses, `runs` for a task from its first turn to its end.

mod autopilot;
mod autopilot_areas;
mod autopilot_enforce;
mod autopilot_engines;
mod autopilot_guard;
mod autopilot_held;
mod autopilot_merge;
mod autopilot_risk;
mod autopilot_rules;
mod autopilot_support;
mod autopilot_watch;
mod background;
mod background_daily;
mod background_start;
mod backtest_api;
mod backtest_escapes;
mod bus;
mod bus_flood;
mod bus_guards;
mod bus_review;
mod bus_support;
mod cancel;
mod capacity_api;
mod capacity_held;
mod capacity_hooks;
mod capacity_paused_fix;
mod capacity_sampling;
mod classes;
mod classes_pin;
mod classes_project;
mod confined;
mod core;
mod could_not_start;
mod crew_refusal;
mod draft;
mod escape_check;
mod escape_scan;
mod escapes;
mod fake;
mod filters;
mod flow;
mod flow_board;
mod flow_map;
mod forge_corrections;
mod forge_merge;
mod forge_resolve;
mod hardening;
mod harness;
mod lifecycle;
mod merging_recovery;
mod overrides;
mod protect;
mod provider_facts;
// The checks are shell commands (`touch`, `rm`, `sleep`, a loop), so these tests run where there is a POSIX shell.
#[cfg(unix)]
mod rerun;
mod review;
mod review_controls;
mod review_fix;
mod review_gates;
mod review_publication;
mod review_publication_repair;
mod review_publication_support;
mod review_rules;
mod review_support;
mod runs;
mod scoring;
mod stamp;
mod starts;
mod tier_settings;
mod triggers;
mod verify;
mod worker_identity;
