# Redaction

One list of what Orbyn knows as a secret, the person's own patterns, and the settings that
decide where each applies (Settings › Secrets, W4).

| Module | Owns |
| --- | --- |
| `builtin` | The recognised formats: id, label, example, the class Crew's refusals name, the patterns |
| `custom` | A person's pattern: reading it (text or regular expression), the checks and limits, plain-words errors |
| `redactor` | The compiled detectors and what they do to text: find, spans, `[hidden: label]`, dots |
| `settings` | The stored document, its safe defaults and the per-surface and per-project rules |
| `api` | Reading the view, one change at a time with its audit row, trying a pattern |

## Where it applies

| Surface | Set | Settings can change it |
| --- | --- | --- |
| Crew: H2 scan, ledger rows, evidence, every Crew object | `Redactor::locked`, every format | No, never. It reads no setting |
| Engine failure text and server error text (`agents`) | `Redactor::locked`, on top of their own rules | No |
| Terminal blocks | Formats left on, then the person's patterns | Yes, one switch |
| What agents read | The same, unless the project is switched off | Yes, one switch, and one per project |

Terminal blocks and the agent context are not built yet; `Core::redactor(surface, project)` is the
one door they will use. Until then only Crew and the two failure-text paths apply redaction.

## Rules

- A missing, unreadable or partly unknown stored document means everything is hidden.
- A format turned off, the Terminal switch, the agents switch and a project switched off can only
  widen what a screen or an agent sees. Each change is one transaction with an audit row (`redaction.*`,
  ids and names only, never a pattern's text).
- Patterns: at most 50, 60 characters of name, 200 of pattern. A regular expression is parsed with a
  nesting limit and compiled under a program-size limit. The engine does not backtrack, so a pattern
  cannot run away; look-around and back-references are refused. A pattern that hides 90% or more of
  ordinary text (or of the recent terminal lines the screen sends) is `pattern_too_broad` and is saved
  only with `accept_broad`.
- The settings are stored in the `settings` table under `redaction`. They are not a preference, so the
  account's settings sync never reads or writes them.
