# Account

The settings sync core: the optional Orbyn account's side of keeping personal settings
the same across a person's computers. Phase 1 has no network, sign-in, keychain or route:
the module is compiled in and inactive under the `Off` gate (`CoreConfig::account_gate`),
and only a test or a debug build can drive it, against an adapter shaped like the Engine's
`DeviceApi`. The design is [docs/account.md](../../../../docs/account.md) and
[its threat model](../../../../docs/account-threat-model.md).

| Module | Owns |
| --- | --- |
| `device_api` | The `DeviceApi` adapter trait (push and pull per zone, status, notices) and the client's error set |
| `merge` | The record, the hybrid logical clock and the one merge function that `orbyn-sync` replaces |
| `settings_record` | The `engineering.settings` record: the 20 synced keys, set elements, keybinding fields, name hashing and limits |
| `policy` | What a peer may not make this device do: keybinding commands and dialog keys, per-adapter model options, branch prefix |
| `apply` | The allowlist-only apply plan; every other field is kept and passed on, never applied |
| `sync` | One worker per zone: pull, merge, apply, upload, backoff and the error table |
| `status` | Status values and their `DataChanged` events (`account`, `account:conflicts`) |
| `fake` | A fake `DeviceApi` for tests (test builds only) |

The local copy of each record, the outbox, the cursor and the conflicts are durable state, so
they live with the rest in [`store::account_sync`](../store/account_sync.rs) (migration 29).
A preference write queues its fields in the same transaction as the write
(`Store::change_settings`); values that arrive from the account go through
`change_settings_synced`, which queues nothing.

The crate's [group table](../../README.md#groups) lists every group.
