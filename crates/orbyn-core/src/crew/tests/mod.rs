//! Router v0 on fake engines: no process starts. `filters`, `scoring` and `overrides` run the
//! pure router; `stamp` checks drafts and the guards; `core` runs the tool through a real
//! `Core` with scripted engine drivers.

mod core;
mod fake;
mod filters;
mod overrides;
mod scoring;
mod stamp;
