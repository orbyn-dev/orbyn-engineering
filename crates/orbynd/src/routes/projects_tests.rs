//! The folder picker's Windows attribute rule.
//!
//! The rule takes the attributes as an argument instead of reading them, so the same table
//! runs on macOS and Linux, where a hidden, system or junction folder cannot be made. What a
//! real folder reports is covered by `folder_browser_skips_windows_hidden_system_and_junction_folders`
//! in `tests/service.rs`, which runs on Windows only.

use super::projects::attributes_hide_folder;

/// `FILE_ATTRIBUTE_DIRECTORY`: every folder the picker sees carries it.
const DIRECTORY: u32 = 0x10;

#[test]
fn hidden_system_and_reparse_point_folders_are_left_out() {
    for (name, attributes) in [
        ("hidden", DIRECTORY | 0x2),
        ("system", DIRECTORY | 0x4),
        ("junction or link", DIRECTORY | 0x400),
        (
            "a hidden system junction",
            DIRECTORY | 0x2 | 0x4 | 0x400 | 0x20,
        ),
    ] {
        assert!(
            attributes_hide_folder(attributes),
            "{name}: {attributes:#x}"
        );
    }
}

#[test]
fn an_ordinary_folder_stays_in_the_picker() {
    for (name, attributes) in [
        ("a directory", DIRECTORY),
        ("a directory with the archive bit", DIRECTORY | 0x20),
        ("a read-only directory", DIRECTORY | 0x1),
        ("no attributes at all", 0),
    ] {
        assert!(
            !attributes_hide_folder(attributes),
            "{name}: {attributes:#x}"
        );
    }
}
