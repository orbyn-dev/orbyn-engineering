//! The app's synced-settings allowlist and the Engine's never-sync list (`NOT_SYNCED` in the
//! pinned `orbyn-engineering` crate) agree in both directions. The app names one Engine crate,
//! `orbyn-device-client`, which does not re-export `NOT_SYNCED`, so the Engine's answer is read
//! the only way its API gives it: `Settings::get` says `NotSynced` for a key on the list.

use std::collections::BTreeSet;

use orbyn_device_client as dc;

use super::*;

fn engine_refuses(key: &str) -> bool {
    matches!(
        dc::Settings::default().get(key),
        Err(dc::SettingsError::NotSynced)
    )
}

/// The catalog keys outside the allowlist that the Engine never syncs. At `engine-v0.2.4` its
/// list has 28 keys and every one of them is a catalog key of the app.
const ENGINE_NEVER_SYNC: [&str; 28] = [
    "add_project_base_directory",
    "app_update_mode",
    "auto_thread_titles",
    "automatic_git_fetch_minutes",
    "background_activity_profile",
    "branch_naming_mode",
    "custom_models",
    "default_effort",
    "default_engine",
    "default_model",
    "default_model_options",
    "default_permission_mode",
    "default_project_scripts",
    "default_theme_set_at",
    "default_thread_env_mode",
    "disabled_engines",
    "enable_agent_browser_access",
    "enable_agent_device_access",
    "environment_icon",
    "fallback_model",
    "keep_awake_during_runs",
    "observability",
    "prefer_saved_login",
    "provider_health_refresh_minutes",
    "provider_instances",
    "storage_cleanup",
    "text_generation_model",
    "worktree_cleanup",
];

/// The catalog keys outside the allowlist that the Engine would accept but the app keeps on the
/// device anyway. A new catalog key lands in neither list until someone decides which it is.
const APP_ONLY: [&str; 25] = [
    "auto_pull",
    "auto_resume_limited_threads",
    "auto_settle_after_days",
    "auto_settle_keep_pull_requests",
    "auto_settle_on_merge",
    "compact_idle_threads",
    "continue_threads_after_update",
    "crew_digest",
    "crew_turn_budget",
    "enable_provider_update_checks",
    "idle_compaction",
    "new_worktrees_start_from_origin",
    "poll_pull_requests",
    "preview_profiles",
    "project_title_instructions",
    "pull_request_merge_method",
    "remove_agent_credits_on_merge",
    "review_guide_instructions",
    "review_guide_model",
    "review_guides_enabled",
    "snooze_limited_threads",
    "source_control_writing_style",
    "task_skip_near_limit",
    "warm_next_turn",
    "worktree_submodules",
];

/// The catalog keys outside the allowlist that the Engine never syncs, sorted.
pub(super) fn engine_refused_catalog_keys() -> Vec<&'static str> {
    let mut keys: Vec<&'static str> = not_allowlisted()
        .into_iter()
        .filter(|key| engine_refuses(key))
        .collect();
    keys.sort_unstable();
    keys
}

#[test]
fn every_key_the_engine_never_syncs_is_outside_the_allowlist() {
    for key in ENGINE_NEVER_SYNC {
        assert!(
            engine_refuses(key),
            "{key} is not on the Engine's never-sync list"
        );
        assert!(
            !record::is_synced_key(key),
            "{key} is allowlisted but the Engine refuses it"
        );
    }
    assert_eq!(engine_refused_catalog_keys(), ENGINE_NEVER_SYNC.to_vec());
}

#[test]
fn every_allowlisted_key_is_one_the_engine_accepts() {
    for key in record::SYNCED_KEYS {
        assert!(
            !engine_refuses(key),
            "{key} is allowlisted but the Engine never syncs it"
        );
    }
}

#[test]
fn the_non_allowlisted_keys_are_the_named_engine_list_plus_the_named_others() {
    let all: BTreeSet<&str> = not_allowlisted().into_iter().collect();
    let named: BTreeSet<&str> = ENGINE_NEVER_SYNC.into_iter().chain(APP_ONLY).collect();
    assert_eq!(all.len(), 53);
    assert_eq!(named.len(), 53, "a key is named twice");
    assert_eq!(
        all, named,
        "a new catalog key needs a decision: allowlist, Engine list or app-only"
    );
    for key in APP_ONLY {
        assert!(
            !engine_refuses(key),
            "{key} is on the Engine's list: move it to ENGINE_NEVER_SYNC"
        );
    }
}
