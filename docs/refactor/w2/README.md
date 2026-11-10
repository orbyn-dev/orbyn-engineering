# W2 screenshot evidence

Before/after captures for the `styles/thread.css` split, taken with Playwright's
Chromium headless shell against the deterministic mock API.

## Method

- Viewport 1280x820, device scale 1, reduced motion, screenshot animations disabled.
- The mock scenario is reset with `/__mock/scenario/default` before each route.
- The page clock is fixed at `2026-01-15T12:00:00Z`, so relative timestamps are stable.
- Each capture waits for `.main`, the surface's content, `document.fonts.ready`, and a
  500 ms settle.
- Themes come from the saved UI preferences: `light`; `dark` with `darkPreset: null`;
  `dark` with `darkPreset: "dim"`.

Surfaces:

| Name | Route | Covers |
| --- | --- | --- |
| `thread` | `#/threads/t-done` | transcript, markdown, composer, shell chrome |
| `shell` | `#/new` | home, sidebar, title bar, composer |
| `pages` | `#/settings/worktrees` | settings page, sidebar, settings nav |

## Result

Pixel difference (`AE`, differing pixels of 1,049,600):

| Surface | Light | Dark | Dim |
| --- | ---: | ---: | ---: |
| thread | 0 | 2 | 3 |
| shell | 0 | 15 | 5 |
| pages | 0 | 9 | 6 |

A second capture of the unchanged split code gives the renderer's own noise floor:

| Surface | Light | Dark | Dim |
| --- | ---: | ---: | ---: |
| thread | 0 | 4 | 2 |
| shell | 0 | 12 | 0 |
| pages | 0 | 9 | 8 |

Every non-zero difference is an isolated glyph-edge pixel (`0`–`2` pixels remain at a 2%
fuzz), not a moved box, colour or layout change. The split keeps all 784 rules in the same
order and the same entry-import position in `main.tsx`; `styles/thread.css` had 4,497 lines
and the 11 files total 4,489, because the 8 blank separator lines at the new boundaries were
dropped.
