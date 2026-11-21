-- Migration 60: the web no longer offers plan mode, so a thread saved with it on would stay read-only
-- with no way out. Clears the flag on every thread. The column stays for now; runs ignore it.
UPDATE threads SET plan_mode = 0 WHERE plan_mode <> 0;
