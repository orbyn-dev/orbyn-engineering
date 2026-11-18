//! The constants the escape log reads, kept as device config in the existing `settings` table under one key (no new table). The key is
//! not a `pref:` key and not `server_settings`, so the account's settings sync never reads or writes it: it stays on this device.
//! Absent means the defaults; present but unreadable or out of range is `Unreadable` (fail closed), never the defaults. PR 2 adds
//! the person's write; this slice reads.

use orbyn_contracts as wire;
use serde::Deserialize;

use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::store::Store;

/// The settings key of the stored document.
pub const THRESHOLDS_KEY: &str = "crew_escape_thresholds";

/// The defaults: a 60-day report window, a 14-day follow-up window, 10 merges before a rate and 2x the baseline.
pub const DEFAULTS: wire::EscapeThresholds = wire::EscapeThresholds {
    report_days: 60,
    follow_up_days: 14,
    min_merges: 10,
    rate_multiple: 2,
    stored: false,
};

const REPORT_DAYS: std::ops::RangeInclusive<u32> = 1..=365;
const FOLLOW_UP_DAYS: std::ops::RangeInclusive<u32> = 1..=90;
const MIN_MERGES: std::ops::RangeInclusive<u32> = 1..=1000;
const RATE_MULTIPLE: std::ops::RangeInclusive<u32> = 1..=100;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    report_days: u32,
    follow_up_days: u32,
    min_merges: u32,
    rate_multiple: u32,
}

/// Whether every value is inside its range. The same check a stored document gets when it is read.
pub fn validate_thresholds(value: &wire::EscapeThresholds) -> Result<()> {
    let fits = REPORT_DAYS.contains(&value.report_days)
        && FOLLOW_UP_DAYS.contains(&value.follow_up_days)
        && MIN_MERGES.contains(&value.min_merges)
        && RATE_MULTIPLE.contains(&value.rate_multiple);
    if fits {
        Ok(())
    } else {
        Err(CoreError::Invalid(
            "The report window is 1 to 365 days, the follow-up window 1 to 90, the sample 1 to 1000 merges and the multiple 1 to 100."
                .into(),
        ))
    }
}

/// The stored thresholds, or the defaults when nothing is stored.
pub fn stored_thresholds(text: Option<&str>) -> Result<wire::EscapeThresholds> {
    let Some(text) = text else {
        return Ok(DEFAULTS);
    };
    let unreadable = || CoreError::Unreadable("The escape thresholds could not be read.".into());
    let document: Document = serde_json::from_str(text).map_err(|_| unreadable())?;
    let value = wire::EscapeThresholds {
        report_days: document.report_days,
        follow_up_days: document.follow_up_days,
        min_merges: document.min_merges,
        rate_multiple: document.rate_multiple,
        stored: true,
    };
    validate_thresholds(&value).map_err(|_| unreadable())?;
    Ok(value)
}

/// The thresholds in force on this device.
pub fn escape_thresholds(store: &Store) -> Result<wire::EscapeThresholds> {
    let text = store
        .setting(THRESHOLDS_KEY)
        .map_err(|_| CoreError::Unreadable("The escape thresholds could not be read.".into()))?;
    stored_thresholds(text.as_deref())
}

impl Core {
    /// The stored constants, or the defaults. Read-only; the Preview flag is checked first.
    pub fn crew_escape_thresholds(&self) -> Result<wire::EscapeThresholds> {
        self.crew_preview_only()?;
        escape_thresholds(&self.store)
    }
}
