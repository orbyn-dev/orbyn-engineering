# Account

The optional Orbyn account's settings sync core: keeping a person's presentation and
preference settings the same across their computers. Phase 1 builds only the core. There is no
network, keychain or UI here (the sign-in routes are in `orbynd`), and the module is inactive: `CoreConfig::account_gate`
ships `Off`, under which `start_settings_sync` answers `account_disabled`, nothing is queued and no
request goes anywhere. `LocalOnly` exists only in test and debug builds. The design is
[docs/account.md](../../../../docs/account.md) and its
[threat model](../../../../docs/account-threat-model.md); this group follows them exactly.

## What it owns

| Module | Owns |
| --- | --- |
| `device_api` | The `DeviceApi` adapter trait (push and pull per zone, status, live notices) and the client's error set, shaped like the Engine's so the real client can replace it |
| `merge` | The record, the hybrid logical clock and the one merge function that `orbyn-sync`'s merge replaces |
| `settings_record` | The `engineering.settings` record: the 11 synced keys plus keybindings, set elements with removal markers, name hashing and limits |
| `policy` | What a peer may not make this device do: keybinding commands and dialog keys, and `branch_name_prefix` as a git ref part |
| `apply` | The allowlist-only apply plan. Every other field is kept and passed on, never applied |
| `sync` | One worker per zone: pull, merge, apply, upload, backoff and the error table |
| `status` | Status values and their `DataChanged` events (`account`, `account:conflicts`) |
| `api` | `AccountApi` (sign-up, sign-in with the new-device approval calls, devices, sign-out), the redacting `Password`, and the client-error to wire-code table |
| `codes` | The invite and emailed-code alphabet (30 symbols, no `0 O 1 I L U`) and input normalisation |
| `flow` | `AccountCenter`'s actions, the session, the one-use "Confirm it's you" confirmations (within ten minutes) and their `Confirmed` token |
| `flow/approval` | A new device waiting for a trusted one (ten minutes on its own clock), the trusted side (list, refuse, let in), and the first-sync choice (combine, this computer's, the account's) |
| `dev` | An in-memory stand-in for the Engine with throwaway accounts (debug and test builds only) |
| `fake` | A fake `DeviceApi` (debug and test builds only) |

The local copy of each record, the outbox, the cursor and the conflicts are durable state, so they
live with the rest in [`store::account_sync`](../store/account_sync.rs) (migration 29; migration 35 adds the owning account). A preference
write queues its fields in the same transaction as the write (`Store::change_settings`); values that
arrive from the account go through `change_settings_synced`, which queues nothing and applies only
fields not applied before.

## Rules to keep

- A setting that raises cost or widens what an agent may do never syncs
  ([What never syncs](../../../../docs/account.md#what-never-syncs)). Add a key to `SYNCED_KEYS` only
  by a reviewed change to that note.
- Nothing a person does waits for the network. A worker failure ends in a status, never an error to a
  caller.
- No secret is stored here or put in a status, an error or a log.

## How it is tested

The tests run the real worker and store against `fake::FakeDeviceApi`, which can cut the network in
the middle of an upload, lose an answer, fail every call with any client error, and change a zone as
another device would. They cover: offline-first behaviour and the queue draining; two devices merging
and surfacing a lost offline edit; every catalog key outside the allowlist arriving from a peer and
not being applied, asserted by name for the moved and escalating keys; the keybinding and git-ref
rules; stamps from the future; the first sync; record growth and compaction across two devices; and
each row of the client error table.

The crate's [group table](../../README.md#groups) lists every group.
