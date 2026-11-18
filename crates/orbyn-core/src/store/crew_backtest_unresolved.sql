-- Migration 52: additive (slice 4n, real escape source). A scored lane also keeps how many confirmed escapes in its window
-- could not be matched because the files of their fix or revert could not be read: NULL means the lane was not scored,
-- which is not zero. Counts only.
ALTER TABLE crew_backtest_lanes ADD COLUMN unresolved_n INTEGER CHECK(unresolved_n IS NULL OR unresolved_n >= 0);
