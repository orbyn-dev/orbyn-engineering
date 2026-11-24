-- Migration 87: why a watch's template start was refused (docs/crew.md, "Watches"). A change row keeps the fixed
-- code the listener recorded, and only a refused start carries one; an old row keeps NULL. No table rebuild is needed.
ALTER TABLE crew_watch_changes ADD COLUMN reason_code TEXT CHECK(reason_code IS NULL OR reason_code IN ('disk_low','no_engine','folder_untrusted','run_in_progress','not_allowed','crew_gone','refused'));
