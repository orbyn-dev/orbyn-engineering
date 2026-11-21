# Preview and inspector contract (H2, H36, K15)

What the three Beta frame groups of Engineer v192 need from the service: **nothing new**. This page records why, what the existing
routes already carry, and the rules the element inspector keeps so a later route has a home. Rows: H2, H36, K15 in [parity](parity.md).
Frames: `pb-h2-*` (6), `pb-h36-*` (6), `pb-k15-*` (6).

## Proposed routes for Eng 2's R list

**None.** Every behaviour the 18 frames draw runs in the page:

| Group | Needs | Why no route |
| --- | --- | --- |
| H2 custom search address | A saved address and a check | `previewSearchEngine` is a page preference (`savedPrefs`), not a service setting. The address joins it as `previewSearchAddress`. |
| H36 typed size, lock, handles after hide | The existing resize | A typed or locked size is one `POST .../previews/{tab}/resize` with `{width, height}`, the route a drag already uses. The range is the service's 100 to 4096. |
| K15 element inspector | The app's own page | It reads the computed colours of the Orbyn page itself and compares them with the theme tokens. No preview tab, no service. |

Note for the lead: the brief called K15 an inspector "in the Browser panel preview". The frames draw something else: the **Theme editor**
points at the *app's own* elements and finds the theme colour they use. The Browser panel's existing "Pick an element" (annotation chip,
`pickPreviewElement`) is unchanged and is not part of K15.

No Person-class write is defined, so `PENDING_PERSON_ROUTES` is unchanged. No `__ORBYN_PREVIEWS__` mock was needed, so
`assert-no-previews.mjs` gains no marker.

## Rules the code keeps

- **A custom search address is data, never evaluated.** Only `http://` or `https://` with no whitespace and exactly one `%s` is used;
  anything else falls back to DuckDuckGo (`searchAddress.ts`). The words are `encodeURIComponent`-encoded and spliced by index, not by
  `String.replace`, so `$&` in the words is not a replacement pattern. A `javascript:` or `data:` address fails the scheme check.
- **A typed size never reaches the service unchecked.** Whole pixels, 100 to 4096 (`parseSize`); a bad value is shown red and dropped.
- **The inspector reads, it never writes.** It calls `getComputedStyle` and compares colour strings. It sets one attribute on `<html>`
  (`data-inspecting`) while picking and removes it on every exit. The click is stopped before it reaches the app. Changing a colour is
  the editor's list, as before; Reset this colour removes that token from `customColors`.
- **Element data is text.** The name shown for a picked element comes from its `aria-label`, `alt`, `title`, role or tag, capped at 60
  characters, and is rendered as a React text node: never as markup, never as a selector or query to run.
- **Content colours are not the theme.** An element inside `[data-content-colour]` or the terminal (`.xterm`) picks nothing and the
  inspector stays open. A project's colour or a provider's tint is set where it belongs.
- **Previewed pages are untouched.** Nothing here runs script inside a preview tab or with the app's origin on a page the person
  browsed; the Browser panel is read as a picture, as before. No secret is read, stored or sent.

## Limits (not verified against a real window)

- The token match compares resolved colour values, so two tokens with the same value resolve to the first in `THEME_TOKENS` order,
  and an element painted with a colour that merely equals a token is reported as using it. "Other parts that use it" counts distinct
  element kinds (tag plus first class) that share the colour, capped at 5000 elements, not a source-level usage count.
- Settings is hidden by CSS while picking (`html[data-inspecting]`) and the page behind it is shown; what is behind is whatever the
  last route was.
- The Custom row in the engine menu reads "Custom…" because a native `<select>` shows the option's own text when closed.
