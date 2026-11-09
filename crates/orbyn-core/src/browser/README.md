# Browser

The pages a person watches beside a thread, and everything that crosses into or out of them.

| Module | Owns |
| --- | --- |
| `preview` | Preview tabs, each a headless Chrome on its own profile, and the servers a thread started: `Tab` and `PreviewManager` (`mod.rs`), opening (`open`), tab control (`tabs`), the frame loop (`pump`), crash backoff (`crash`), page snapshots (`snapshots`), page actions and expressions (`expressions`), named profiles (`profiles`), local server discovery (`servers`), icon discovery (`favicon`) |
| `preview::bridge` | Copy, file choice and downloads between a page and its viewer |
| `preview::recording` | Bounded JPEG recordings |
| `preview::trace` | Ephemeral, content-free input cues |
| `remote_images` | Bounded remote raster images with pinned DNS |

The old paths `browser::preview_bridge`, `preview_recording` and `preview_trace` still resolve as aliases in `mod.rs`, and the limits and types of `preview` are re-exported from its `mod.rs`. Tests sit in `preview/tests.rs`; the three small modules keep theirs inline.

`remote_images` owns the shared fetch policy for browser-facing remote raster
media. The HTTP adapter exposes it to both thread media and browser history icons;
neither presentation caller owns the fetch policy.

The crate's [group table](../../README.md#groups) lists every group.
