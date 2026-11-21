-- Migration 67: additive (issue #760). A lane keeps the kind it was started as, so a report can say whether it is a
-- review or a check. NULL means the lane was recorded before lanes kept their kind, which is never guessed: an old row
-- reads no kind rather than a review. The kind is written once, when the run is recorded, and never changes.
ALTER TABLE crew_backtest_lanes ADD COLUMN kind TEXT CHECK(kind IS NULL OR kind IN ('review', 'check'));
