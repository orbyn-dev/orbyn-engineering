-- Migration 72: an environment key's presence is recorded beside its name, so listing sets no longer reads the keychain for
-- every key. NULL means the row came from before this and has not been checked yet; 0 means no value is stored; 1 means a
-- value was stored. This is not a value column, on purpose: values live in the OS keychain only (hard rule 1).
ALTER TABLE library_env_keys ADD COLUMN is_set INTEGER;
