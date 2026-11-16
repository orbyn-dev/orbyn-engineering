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

/// Engines whose detail page is not built yet: their rows carry no Open.
const NO_DETAIL: &[&str] = &["cursor", "fx"];

/// Known ACP engines with a frame of their own. The other unverified ones are listed together.
const OWN_ROW: &[&str] = &["cursor", "fx"];

/// What is not built of an engine that otherwise starts, by engine id.
const PARTLY_NOT_BUILT: &[(&str, &str)] = &[("cline", "its effort flag is not built")];

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
    if cfg!(windows) {
        // Per-user and machine-wide installs. These Windows paths are not verified on a real install.
        for (variable, below) in [("LOCALAPPDATA", Some("Programs")), ("ProgramFiles", None)] {
            if let Some(root) = std::env::var_os(variable) {
                let root = PathBuf::from(root);
                folders.push(below.map_or(root.clone(), |below| root.join(below)));
            }
        }
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
    if OWN_ROW.contains(&family) {
        return Some(if family == "fx" {
            "Starts over ACP. Its Auto mode is described as writing without asking.".to_owned()
        } else {
            "Starts over ACP. Not verified on a real install.".to_owned()
        });
    }
    // The rest read as their program, and what is not built of it.
    let program = known_engine(family).map_or(family, |known| known.programs[0]);
    Some(
        match PARTLY_NOT_BUILT.iter().find(|(id, _)| *id == family) {
            Some((_, part)) => format!("{program} · {part}"),
            None => program.to_owned(),
        },
    )
}

fn entry_of(status: &EngineStatus) -> ProviderEntry {
    let facts = ProviderFacts::of(status);
    let mut labels = facts.labels();
    // Only an engine that can start is verified or not; a missing or broken one says that alone.
    if facts.installed() && !labels.contains(&ProviderLabel::CantStart) {
        if not_verified(status) {
            labels.push(ProviderLabel::NotVerified);
        }
        if PARTLY_NOT_BUILT.iter().any(|(id, _)| *id == family(status)) {
            labels.push(ProviderLabel::Planned);
        }
    }
    let group = if labels.contains(&ProviderLabel::NotFound) {
        ProviderGroup::NotFound
    } else if labels.contains(&ProviderLabel::CantStart) {
        ProviderGroup::CantStart
    } else if labels.contains(&ProviderLabel::NotVerified) {
        if OWN_ROW.contains(&family(status)) {
            ProviderGroup::Found
        } else {
            ProviderGroup::More
        }
    } else {
        ProviderGroup::InUse
    };
    let note = note(status, &facts, &labels);
    ProviderEntry {
        id: status.id.clone(),
        name: status.name.clone(),
        group,
        labels,
        note,
        registered: true,
        openable: !NO_DETAIL.contains(&family(status)),
    }
}

/// A program found under an engine's name that is another CLI: Orbyn refuses it.
pub struct Refused {
    pub id: &'static str,
    pub path: String,
}

/// The refused programs on this machine, read from disk.
pub fn refused_programs() -> Vec<Refused> {
    super::engines::known::KNOWN_ENGINES
        .iter()
        .filter(|known| known.id == "grok")
        .filter_map(|known| {
            known.refused().map(|path| Refused {
                id: known.id,
                path: path.to_string_lossy().into_owned(),
            })
        })
        .collect()
}

/// The list for these statuses, and the app-only engines found on this machine.
pub fn list(statuses: &[EngineStatus], apps: &[&str], refused: &[Refused]) -> ProviderList {
    let mut providers: Vec<ProviderEntry> = statuses.iter().map(entry_of).collect();
    for one in refused {
        if let Some(known) = known_engine(one.id)
            && !statuses.iter().any(|status| status.id == one.id)
        {
            providers.push(ProviderEntry {
                id: one.id.to_owned(),
                name: known.name.to_owned(),
                group: ProviderGroup::CantStart,
                labels: vec![ProviderLabel::NotFound],
                note: Some(format!(
                    "Found a program named {} at {}, but it is not xAI’s Grok CLI. Orbyn does not use it. Get xAI’s own Grok Build from xAI’s official page.",
                    known.programs[0], one.path
                )),
                registered: false,
                openable: false,
                })
        }
    }
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
                openable: false,
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
            openable: false,
        });
    }
    // Stable by group, keeping the registry's own order inside each.
    providers.sort_by_key(|entry| match entry.group {
        ProviderGroup::InUse => 0,
        ProviderGroup::Found => 1,
        ProviderGroup::More => 2,
        ProviderGroup::CantStart => 3,
        ProviderGroup::NotFound => 4,
        ProviderGroup::Planned => 5,
    });
    ProviderList { providers }
}

#[cfg(test)]
mod tests;
