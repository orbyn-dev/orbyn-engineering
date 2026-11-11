//! The record and its merge: one value per field, each with a hybrid logical clock
//! stamp, and the larger stamp wins. [`merge_records`] is the only entry point, so
//! `orbyn-sync`'s merge can replace this module's body when the Engine crates land.
//! The clock here is a small local one for the same reason.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

/// How far ahead of the local clock a remote stamp may pull it. A device with a wrong
/// clock, or a hostile one, must not push every later local edit years into the future.
const MAX_DRIFT_MS: u64 = 24 * 60 * 60 * 1000;

/// When a field was written: wall time in milliseconds, a counter for writes in the
/// same millisecond, and the writing device. Field order is the comparison order.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Stamp {
    pub wall_ms: u64,
    pub counter: u32,
    pub device: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub value: Value,
    pub stamp: Stamp,
}

/// A zone's record: every field by name, the ones this version does not know too.
pub type Record = BTreeMap<String, Field>;

/// The local hybrid logical clock: the newest stamp seen or made, so the next one is
/// always later.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hlc {
    pub wall_ms: u64,
    pub counter: u32,
}

impl Hlc {
    pub fn new(wall_ms: u64, counter: u32) -> Self {
        Self { wall_ms, counter }
    }

    /// The next stamp for an edit made at `at_ms`. An edit queued while the app was
    /// closed keeps its own time unless the clock already passed it.
    pub fn tick_at(&mut self, at_ms: u64) -> (u64, u32) {
        if at_ms > self.wall_ms {
            self.wall_ms = at_ms;
            self.counter = 0;
        } else {
            self.counter = self.counter.saturating_add(1);
        }
        (self.wall_ms, self.counter)
    }

    /// Moves the clock past a stamp seen on another device, within the drift limit.
    pub fn observe(&mut self, wall_ms: u64, counter: u32, now_ms: u64) {
        let wall_ms = wall_ms.min(now_ms.saturating_add(MAX_DRIFT_MS));
        if (wall_ms, counter) > (self.wall_ms, self.counter) {
            self.wall_ms = wall_ms;
            self.counter = counter;
        }
    }
}

/// The merge of a local and a remote record.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Merged {
    /// Every field, the winner of each.
    pub record: Record,
    /// Fields where the remote value won and differs from the local one. A local
    /// value that was still waiting to upload and lost is a conflict.
    pub remote_won: BTreeSet<String>,
    /// Fields the remote record lacks or holds an older stamp for: what must upload.
    pub local_ahead: BTreeSet<String>,
}

/// Per-field last writer wins: the larger stamp, then the larger canonical JSON.
pub fn merge_records(local: &Record, remote: &Record) -> Merged {
    let mut merged = Merged::default();
    let names: BTreeSet<&String> = local.keys().chain(remote.keys()).collect();
    for name in names {
        let (kept, from_remote) = match (local.get(name), remote.get(name)) {
            (Some(mine), Some(theirs)) if mine == theirs => (mine, false),
            (Some(mine), Some(theirs)) => {
                if remote_wins(mine, theirs) {
                    (theirs, true)
                } else {
                    (mine, false)
                }
            }
            (Some(mine), None) => (mine, false),
            (None, Some(theirs)) => (theirs, true),
            (None, None) => continue,
        };
        merged.record.insert(name.clone(), kept.clone());
        if from_remote {
            merged.remote_won.insert(name.clone());
        } else if remote.get(name) != Some(kept) {
            merged.local_ahead.insert(name.clone());
        }
    }
    merged
}

fn remote_wins(local: &Field, remote: &Field) -> bool {
    match remote.stamp.cmp(&local.stamp) {
        std::cmp::Ordering::Equal => canonical(&remote.value) > canonical(&local.value),
        order => order.is_gt(),
    }
}

/// JSON with object keys in order, so two devices break a tie the same way.
pub fn canonical(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys
                .into_iter()
                .map(|key| format!("{}:{}", Value::String(key.clone()), canonical(&map[key])))
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(canonical).collect();
            format!("[{}]", parts.join(","))
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn field(value: Value, wall_ms: u64, counter: u32, device: &str) -> Field {
        Field {
            value,
            stamp: Stamp {
                wall_ms,
                counter,
                device: device.into(),
            },
        }
    }

    fn record(fields: &[(&str, Field)]) -> Record {
        fields
            .iter()
            .map(|(name, field)| ((*name).to_owned(), field.clone()))
            .collect()
    }

    #[test]
    fn the_larger_stamp_wins_in_either_order() {
        let old = record(&[("a", field(json!(1), 10, 0, "x"))]);
        let new = record(&[("a", field(json!(2), 11, 0, "y"))]);
        for (local, remote) in [(&old, &new), (&new, &old)] {
            let merged = merge_records(local, remote);
            assert_eq!(merged.record["a"].value, json!(2));
        }
        let counter = record(&[("a", field(json!(3), 10, 1, "x"))]);
        assert_eq!(merge_records(&old, &counter).record["a"].value, json!(3));
        let device = record(&[("a", field(json!(4), 10, 0, "z"))]);
        assert_eq!(merge_records(&old, &device).record["a"].value, json!(4));
    }

    #[test]
    fn a_tie_breaks_by_canonical_json_on_both_sides() {
        let one = record(&[("a", field(json!({"b": 1, "a": 2}), 5, 0, "x"))]);
        let two = record(&[("a", field(json!({"a": 2, "b": 2}), 5, 0, "x"))]);
        let left = merge_records(&one, &two).record;
        let right = merge_records(&two, &one).record;
        assert_eq!(left, right);
        assert_eq!(left["a"].value, json!({"a": 2, "b": 2}));
    }

    #[test]
    fn it_reports_what_came_from_the_remote_and_what_must_upload() {
        let local = record(&[
            ("mine", field(json!(1), 5, 0, "x")),
            ("both", field(json!(1), 5, 0, "x")),
            ("newer", field(json!(9), 9, 0, "x")),
        ]);
        let remote = record(&[
            ("theirs", field(json!(1), 1, 0, "y")),
            ("both", field(json!(2), 6, 0, "y")),
            ("newer", field(json!(2), 6, 0, "y")),
        ]);
        let merged = merge_records(&local, &remote);
        assert_eq!(
            merged.remote_won,
            BTreeSet::from(["theirs".to_owned(), "both".to_owned()])
        );
        assert_eq!(
            merged.local_ahead,
            BTreeSet::from(["mine".to_owned(), "newer".to_owned()])
        );
        assert_eq!(merged.record.len(), 4);
        // Merging the result again changes nothing.
        let again = merge_records(&merged.record, &merged.record);
        assert!(again.remote_won.is_empty() && again.local_ahead.is_empty());
    }

    #[test]
    fn the_clock_never_goes_backwards_and_keeps_an_old_edits_time() {
        let mut clock = Hlc::default();
        assert_eq!(clock.tick_at(1000), (1000, 0));
        assert_eq!(clock.tick_at(1000), (1000, 1));
        // An edit queued earlier than the clock still gets a later stamp.
        assert_eq!(clock.tick_at(900), (1000, 2));
        assert_eq!(clock.tick_at(2000), (2000, 0));
    }

    #[test]
    fn a_remote_stamp_moves_the_clock_but_not_far_into_the_future() {
        let mut clock = Hlc::new(1000, 0);
        clock.observe(5000, 3, 4000);
        assert_eq!(clock.tick_at(4000), (5000, 4));
        let mut skewed = Hlc::new(1000, 0);
        skewed.observe(u64::MAX, 0, 4000);
        assert_eq!(skewed.wall_ms, 4000 + MAX_DRIFT_MS);
    }
}
