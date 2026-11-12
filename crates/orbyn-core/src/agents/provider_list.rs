//! The grouped provider list: every engine Orbyn found, with the labels that say what it has
//! not seen or cannot control, plus the rows for engines it cannot start or has not built.
//! State comes from [`ProviderFacts`], the same facts the crew router filters read.

use std::path::PathBuf;

use orbyn_contracts::{
    EngineStatus, EngineTier, ProviderEntry, ProviderGroup, ProviderLabel, ProviderList,
};

use super::engines::known::known_engine;
use super::provider_facts::ProviderFacts;

/// Known ACP engines Orbyn has run on a real install, which is all the others lack. Only
/// OpenCode 2.2.5 ran real turns through `orbynd` (docs/architecture.md, engines).
const SEEN_ON_REAL_INSTALL: &[&str] = &["opencode"];

/// What is not built of an engine that otherwise starts, by engine id.
const PARTLY_NOT_BUILT: &[(&str, &str)] = &[("cline", "Its effort flag is not built.")];

/// An engine Orbyn has no driver for. Shown so the gap is visible.
struct Planned {
    id: &'static str,
    name: &'static str,
    note: &'static str,
}

const PLANNED: &[Planned] = &[
    Planned {
        id: "pi",
        name: "Pi",
        note: "Pi speaks its own protocol, not ACP. Orbyn has no driver for it yet.",
    },
    Planned {
        id: "muse",
        name: "Muse Code",
        note: "Muse Code speaks its own protocol, not ACP. Orbyn has no driver for it yet.",
    },
    Planned {
        id: "zcode",
        name: "ZCode",
        note: "Uses a private protocol. Not built.",
    },
];

/// An app that is installed but holds no program Orbyn can start as an agent.
struct AppOnly {
    id: &'static str,
    name: &'static str,
    /// Where each platform puts it, relative to the folders named in [`app_folders`].
    paths: &'static [&'static str],
}

const APP_ONLY: &[AppOnly] = &[AppOnly {
    id: "antigravity",
    name: "Antigravity",
    paths: &["Antigravity.app", "Antigravity/Antigravity.exe"],
}];

/// The folders an app is looked for in, per platform.
fn app_folders() -> Vec<PathBuf> {
    let mut folders = Vec::new();
    if cfg!(target_os = "macos") {
        folders.push(PathBuf::from("/Applications"));
        if let Some(home) = std::env::var_os("HOME") {
            folders.push(PathBuf::from(home).join("Applications"));
        }
    }
    if cfg!(windows)
        && let Some(local) = std::env::var_os("LOCALAPPDATA")
    {
        folders.push(PathBuf::from(local).join("Programs"));
    }
    folders
}

/// The ids of app-only engines found in `folders`.
pub fn apps_found(folders: &[PathBuf]) -> Vec<&'static str> {
    APP_ONLY
        .iter()
        .filter(|app| {
            folders
                .iter()
                .any(|folder| app.paths.iter().any(|path| folder.join(path).exists()))
        })
        .map(|app| app.id)
        .collect()
}

/// The installed apps, read from disk.
pub fn installed_apps() -> Vec<&'static str> {
    apps_found(&app_folders())
}

fn family(status: &EngineStatus) -> &str {
    status.instance_of.as_deref().unwrap_or(&status.id)
}

/// Starts over ACP from a CLI of its own and was only tried with a test agent.
fn not_verified(status: &EngineStatus) -> bool {
    let family = family(status);
    status.tier == EngineTier::Acp
        && known_engine(family).is_some()
        && !SEEN_ON_REAL_INSTALL.contains(&family)
}

fn note(status: &EngineStatus, facts: &ProviderFacts, labels: &[ProviderLabel]) -> Option<String> {
    if labels.contains(&ProviderLabel::CantStart) || labels.contains(&ProviderLabel::LimitedSupport)
    {
        return facts
            .broken_note()
            .or(facts.limited_note())
            .map(str::to_owned);
    }
    if !labels.contains(&ProviderLabel::NotVerified) {
        return None;
    }
    let family = family(status);
    let detail = if let Some((_, part)) = PARTLY_NOT_BUILT.iter().find(|(id, _)| *id == family) {
        format!(" {part}")
    } else if family == "fx" {
        " Its Auto mode is described as writing without asking.".to_owned()
    } else if labels.contains(&ProviderLabel::Unmanaged) {
        " It cannot ask before it edits.".to_owned()
    } else {
        " Not verified on a real install.".to_owned()
    };
    Some(format!("Starts over ACP.{detail}"))
}

fn entry_of(status: &EngineStatus) -> Option<ProviderEntry> {
    let facts = ProviderFacts::of(status);
    // An engine that is not found does not appear.
    if !facts.installed() {
        return None;
    }
    let mut labels = facts.labels();
    if not_verified(status) {
        labels.push(ProviderLabel::NotVerified);
    }
    if PARTLY_NOT_BUILT.iter().any(|(id, _)| *id == family(status)) {
        labels.push(ProviderLabel::Planned);
    }
    let group = if labels.contains(&ProviderLabel::CantStart) {
        ProviderGroup::CantStart
    } else if labels.contains(&ProviderLabel::NotVerified) {
        ProviderGroup::Found
    } else {
        ProviderGroup::InUse
    };
    let note = note(status, &facts, &labels);
    Some(ProviderEntry {
        id: status.id.clone(),
        name: status.name.clone(),
        group,
        labels,
        note,
        registered: true,
    })
}

/// The list for these statuses, and the app-only engines found on this machine.
pub fn list(statuses: &[EngineStatus], apps: &[&str]) -> ProviderList {
    let mut providers: Vec<ProviderEntry> = statuses.iter().filter_map(entry_of).collect();
    for app in APP_ONLY {
        if apps.contains(&app.id) && !statuses.iter().any(|status| status.id == app.id) {
            providers.push(ProviderEntry {
                id: app.id.to_owned(),
                name: app.name.to_owned(),
                group: ProviderGroup::CantStart,
                labels: vec![ProviderLabel::CantStart],
                note: Some(format!(
                    "{} is installed, but it has no ACP server Orbyn can start.",
                    if cfg!(target_os = "macos") {
                        "Antigravity.app"
                    } else {
                        "Antigravity"
                    }
                )),
                registered: false,
            });
        }
    }
    for planned in PLANNED {
        providers.push(ProviderEntry {
            id: planned.id.to_owned(),
            name: planned.name.to_owned(),
            group: ProviderGroup::Planned,
            labels: vec![ProviderLabel::Planned],
            note: Some(planned.note.to_owned()),
            registered: false,
        });
    }
    // Stable by group, keeping the registry's own order inside each.
    providers.sort_by_key(|entry| match entry.group {
        ProviderGroup::InUse => 0,
        ProviderGroup::Found => 1,
        ProviderGroup::CantStart => 2,
        ProviderGroup::Planned => 3,
    });
    ProviderList { providers }
}

#[cfg(test)]
mod tests;
