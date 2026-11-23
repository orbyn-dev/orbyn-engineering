//! `low_disk_reason` over the one volume a worker start checks: a volume below the floor is
//! refused by name, and a volume whose free space cannot be read never blocks a start.

use crate::crew::start::low_disk_reason;

const FLOOR: u64 = 2 * 1024 * 1024 * 1024;
const ROOMY: u64 = FLOOR + 1;

#[test]
fn a_volume_above_the_floor_starts() {
    assert_eq!(
        low_disk_reason(&[("the project folder", Some(ROOMY))], FLOOR),
        None
    );
}

#[test]
fn a_volume_below_the_floor_is_named_in_the_refusal() {
    let reason = low_disk_reason(&[("the project folder", Some(FLOOR - 1))], FLOOR)
        .expect("the project folder is below the floor");
    assert!(reason.contains("the project folder"), "{reason}");
    assert!(reason.contains("needed"), "{reason}");
}

#[test]
fn an_unreadable_volume_never_blocks() {
    assert_eq!(
        low_disk_reason(&[("the project folder", None)], FLOOR),
        None
    );
}
