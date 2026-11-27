//! The apps Snap Shot never captures. The built-in list cannot shrink; the person's list only adds to it.

use super::settings::Settings;

const BUNDLE_IDS: &[&str] = &[
    "com.1password.1password",
    "com.agilebits.onepassword7",
    "com.agilebits.onepassword-osx",
    "com.bitwarden.desktop",
    "com.dashlane.dashlane",
    "com.lastpass.lastpass",
    "org.keepassxc.keepassxc",
    "com.apple.keychainaccess",
    "com.apple.Passwords",
];

const APP_NAMES: &[&str] = &[
    "1Password",
    "Bitwarden",
    "Dashlane",
    "LastPass",
    "KeePass",
    "KeePassXC",
    "Keychain Access",
    "Passwords",
];

fn listed(value: &str, list: &[&str]) -> bool {
    list.iter().any(|entry| entry.eq_ignore_ascii_case(value))
}

pub fn is_excluded(app_id: &str, app_name: &str, own_bundle_id: &str, settings: &Settings) -> bool {
    if !own_bundle_id.is_empty() && app_id.eq_ignore_ascii_case(own_bundle_id) {
        return true;
    }
    if listed(app_id, BUNDLE_IDS) || listed(app_name, APP_NAMES) {
        return true;
    }
    settings
        .snapshot_excluded_apps
        .iter()
        .any(|entry| entry.eq_ignore_ascii_case(app_id) || entry.eq_ignore_ascii_case(app_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_stays_on_the_list() {
        let settings = Settings::default();
        for &id in BUNDLE_IDS {
            assert!(is_excluded(id, "", "own", &settings), "{id}");
        }
        for &name in APP_NAMES {
            assert!(is_excluded("unknown", name, "own", &settings), "{name}");
        }
    }

    #[test]
    fn built_ins_match_ids_and_names_without_case() {
        let settings = Settings::default();
        assert!(is_excluded(
            "com.bitwarden.desktop",
            "Bitwarden",
            "own",
            &settings
        ));
        assert!(is_excluded(
            "COM.BITWARDEN.DESKTOP",
            "whatever",
            "own",
            &settings
        ));
        assert!(is_excluded("unknown", "pAsSwOrDs", "own", &settings));
        assert!(is_excluded("unknown", "keychain access", "own", &settings));
        assert!(!is_excluded("com.apple.Notes", "Notes", "own", &settings));
    }

    #[test]
    fn this_app_is_always_excluded() {
        let settings = Settings::default();
        assert!(is_excluded(
            "COM.KINQSRADIO.ORBYN.ENGINEERING",
            "Orbyn Engineering",
            "com.kinqsradio.orbyn.engineering",
            &settings
        ));
        assert!(!is_excluded("", "", "", &settings));
    }

    #[test]
    fn the_person_can_only_add() {
        let mut settings = Settings::default();
        settings.snapshot_excluded_apps = vec!["com.example.secret".to_owned()];
        assert!(is_excluded(
            "com.example.secret",
            "Secret",
            "own",
            &settings
        ));
        assert!(!is_excluded("whatever", "SECRET", "own", &settings));
        settings.snapshot_excluded_apps = vec!["Secret".to_owned()];
        assert!(is_excluded("whatever", "SECRET", "own", &settings));
        settings.snapshot_excluded_apps = vec!["com.example.secret".to_owned()];
        assert!(is_excluded(
            "com.1password.1password",
            "1Password",
            "own",
            &settings
        ));
        settings.snapshot_excluded_apps = vec!["com.example.other".to_owned()];
        assert!(is_excluded(
            "com.dashlane.dashlane",
            "Dashlane",
            "own",
            &settings
        ));
    }
}
