//! The daily digest's send time (issue #806). A person stores a local time and the UTC offset their
//! local time was in when they saved it (Settings › Crew, `POST /api/crew/autopilot/digest/schedule/set`,
//! a Person route); the watch tick reads it and resolves the one local day it may fire on.
//!
//! The stored shape is the scheduled-tasks shape (`{time, utc_offset_minutes}`,
//! `orbyn-contracts`), and local time resolves the same way: a fixed offset kept from the save. It is
//! one person-level value, so it lives in the settings preferences (`crew_digest`) and needs no
//! migration and no table.
//!
//! No offset is stored until a person first saves, and then the digest sends nothing. With no offset
//! "17:00 local" cannot be resolved, and a guess (UTC, or the service's own time zone) could send
//! early; the owner ruled that out. The Settings row shows the 17:00 default, so the person can save
//! a time and the digest starts.

use std::collections::BTreeMap;

use orbyn_contracts as wire;
use serde_json::json;

use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::settings::preferences::{CREW_DIGEST, digest_schedule};

/// The local day (days since the epoch in the stored offset) the digest may fire on, or `None` when
/// the stored time has not come round yet or no offset is stored.
pub(crate) fn digest_due_day(value: &serde_json::Value, now: i64) -> Option<i64> {
    let (hour, minute, offset) = digest_schedule(value)?;
    let offset = offset?;
    let local = now + i64::from(offset) * 60;
    let at = i64::from(hour) * 3600 + i64::from(minute) * 60;
    if local.rem_euclid(86_400) >= at {
        Some(local.div_euclid(86_400))
    } else {
        None
    }
}

impl Core {
    /// The daily digest's send time as it stands. The default is 17:00 with no stored offset, and such
    /// a schedule sends nothing until a person saves one.
    pub fn crew_digest_schedule(&self) -> Result<wire::AutopilotDigestSchedule> {
        self.crew_preview_only()?;
        let value = self.preference(CREW_DIGEST, None)?;
        let Some((hour, minute, offset)) = digest_schedule(&value) else {
            return Err(CoreError::Invalid(
                "The stored digest time is not valid.".into(),
            ));
        };
        Ok(wire::AutopilotDigestSchedule {
            time: format!("{hour:02}:{minute:02}"),
            utc_offset_minutes: offset,
        })
    }

    /// A person sets it, with the offset their local time was in when they saved it. The preference
    /// catalog validates the shape and stores it like every other setting.
    pub fn crew_set_digest_schedule(
        &self,
        input: &wire::AutopilotDigestScheduleInput,
    ) -> Result<wire::AutopilotDigestSchedule> {
        self.crew_preview_only()?;
        let value = json!({
            "time": input.time,
            "utc_offset_minutes": input.utc_offset_minutes,
        });
        let mut values = BTreeMap::new();
        values.insert(CREW_DIGEST.to_owned(), value);
        self.set_preferences(None, &values)?;
        self.crew_digest_schedule()
    }

    /// The local day of `now` in the stored offset, once the stored time has passed; `None` before it
    /// and while no offset is stored. The digest is due at most once on the day this names.
    pub(crate) fn crew_digest_due_day(&self, now: i64) -> Result<Option<i64>> {
        Ok(digest_due_day(&self.preference(CREW_DIGEST, None)?, now))
    }
}

#[cfg(test)]
mod tests {
    use super::digest_due_day;
    use serde_json::json;

    /// 2027-01-15T08:00:00Z, a Friday.
    const NOW: i64 = 1_800_000_000;

    fn saved(time: &str, offset: i32) -> serde_json::Value {
        json!({ "time": time, "utc_offset_minutes": offset })
    }

    #[test]
    fn nothing_is_due_before_the_stored_time_or_without_an_offset() {
        assert_eq!(digest_due_day(&json!({ "time": "17:00" }), NOW), None);
        assert_eq!(digest_due_day(&json!({}), NOW), None);
        assert_eq!(
            digest_due_day(&saved("17:00", 0), NOW),
            None,
            "08:00 UTC is before 17:00"
        );
    }

    #[test]
    fn the_digest_is_due_at_the_stored_time_and_names_that_local_day() {
        let at = digest_due_day(&saved("17:00", 0), NOW + 9 * 3600);
        assert_eq!(at, Some((NOW + 9 * 3600).div_euclid(86_400)));
        assert_eq!(
            digest_due_day(&saved("08:00", 0), NOW),
            Some(NOW.div_euclid(86_400))
        );
        assert_eq!(
            digest_due_day(&saved("07:59", 0), NOW),
            Some(NOW.div_euclid(86_400))
        );
    }

    #[test]
    fn the_local_day_follows_the_saved_offset_not_utc() {
        // UTC+11: 08:00 UTC is 19:00 local, so 17:00 local has passed and the day is the local one.
        let local = NOW + 11 * 3600;
        assert_eq!(
            digest_due_day(&saved("17:00", 660), NOW),
            Some(local.div_euclid(86_400))
        );
        // UTC-10: 08:00 UTC is 22:00 the day before, so 23:00 local has not come round yet.
        assert_eq!(digest_due_day(&saved("23:00", -600), NOW), None);
        // One minute before the saved time on the same local day is still nothing.
        assert_eq!(digest_due_day(&saved("19:01", 660), NOW), None);
    }

    #[test]
    fn the_next_local_day_is_a_different_day() {
        let today = digest_due_day(&saved("17:00", 0), NOW + 9 * 3600).unwrap();
        let tomorrow = digest_due_day(&saved("17:00", 0), NOW + 9 * 3600 + 86_400).unwrap();
        assert_eq!(tomorrow, today + 1);
    }
}
