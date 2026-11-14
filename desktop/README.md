# desktop

The thin Tauri 2 shell. It starts `orbynd`, loads the bundled UI and does only
what the operating system must do: windows, menus, tray, links, file dialogs and
app updates. Its rules are in
[docs/architecture.md](../docs/architecture.md#desktop-shell).

## Contract

- It runs `orbynd` with `--parent-stdin` and talks to it over that process's
  stdin and stdout. The product UI calls the shell in one place only: `crew_presence`, which
  shows the native dialog for approving one held Crew command
  (`src/presence.rs`).
- The product UI comes from `build/ui`, which `npm run prepare-bundle` fills.
  `loader/index.html` is the embedded startup page, the only document allowed to
  call `restart_core`. The product UI restarts the service through the service's
  `restart_service` shell action instead. A service that crashes is started again
  once by itself (`src/restart_policy.rs`).

## Consumers

People, through the installed app. Nothing in this repository imports it.

## Testing

`cargo test` in `src-tauri` runs the shell's unit tests with no window. The approval dialog
itself cannot run there; try it by hand in a built app (`npm run dev` in `desktop`) with a held
command. A fresh checkout needs `build/ui`, `build/library` and placeholder
`src-tauri/binaries/orbynd-<target>` and `orbyn-<target>` files (all ignored by Git) before it
compiles; `npm run prepare-bundle` makes the real ones.

## Known limitations

- `src-tauri` is outside the root Cargo workspace, so the root
  `cargo test --workspace` does not build it. CI's desktop jobs do, on macOS and
  Windows.
