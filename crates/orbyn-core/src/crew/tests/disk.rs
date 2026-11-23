//! `low_disk_reason` over the one volume a worker start checks: a volume below the floor is
//! refused with its free space, and a volume whose free space cannot be read never blocks a
//! start. `free_text` is the unit the refusal speaks in.

use crate::crew::start::{free_text, low_disk_reason};

const GIB: u64 = 1024 * 1024 * 1024;

#[test]
fn free_space_is_written_as_whole_mebibytes_or_tenths_of_a_gibibyte() {
    assert_eq!(free_text(3_400_000_000), "3.1 GB");
    assert_eq!(free_text(GIB), "1.0 GB");
    assert_eq!(free_text(640 * 1024 * 1024), "640 MB");
    assert_eq!(free_text(GIB - 1), "1023 MB");
}

#[test]
fn a_volume_below_the_floor_is_refused_with_its_free_space() {
    assert_eq!(
        low_disk_reason(Some(3_400_000_000), 5).unwrap(),
        "Not started: 3.1 GB free, Crew needs 5 GB. Free space and try again."
    );
}

#[test]
fn a_volume_with_free_space_equal_to_the_floor_starts() {
    assert_eq!(low_disk_reason(Some(5 * GIB), 5), None);
    assert_eq!(low_disk_reason(Some(20 * GIB), 5), None);
}

#[test]
fn an_unreadable_volume_never_blocks() {
    assert_eq!(low_disk_reason(None, 5), None);
}
