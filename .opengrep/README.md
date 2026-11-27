# Opengrep rules

Starter static-analysis rules for this project, run by the **advisory** workflow
[opengrep.yml](../.github/workflows/opengrep.yml). It is not a required check, never fails a pull request on a
finding, and **does not replace CodeQL**.

## What is here

- `rules/rust.yaml`, `rules/web.yaml`: nine Rust and seven TypeScript or JavaScript rules. The starter set was written
  from the project's risk list (process spawning and shells, SQL built with `format!`, TLS verification, `unsafe`,
  secret-named values in logs, `eval`, unsafe HTML, secrets in browser storage, reflected HTML from a request URL,
  first-occurrence-only `replace`, incomplete HTML-comment stripping). Four rules cover a CodeQL query the starter set
  did not:
  - `orbyn-rust-request-path-to-fs` for `rust/path-injection`: the request's own URL text reaching a filesystem path,
    with this repository's `resolve` confinement as the sanitizer.
  - `orbyn-rust-sql-built-query-reaches-execute` for `rust/sql-injection`: a `format!`-built statement reaching a query
    call through a variable, which the inline `orbyn-rust-sql-format` rule cannot see.
  - `orbyn-rust-secret-field-in-log` for `rust/cleartext-logging`: a secret-named field reaching a log call through
    another value; `redact`, `redacted_tail` and length-only reads are sanitizers.
  - `orbyn-js-regex-escape-skips-backslash` for `js/incomplete-sanitization`: `replace(/[…]/g, "\\$&")`, in either
    quote style, whose character class matches no backslash of its own — not negated, and without a `\W`, `\S` or `\D`
    shorthand, a `\x5c` or `\u005c` escape, or an octal `\134` (`web/src/styles/tokens.test.ts:165` was the known case;
    it is fixed). A `\p{…}` property escape is outside the rule.
- `tests/`: a matching case and a safe look-alike for every rule. A rule is unproven without both. The workflow runs
  them first.

## Run it locally

Build the engine from the pinned source with the library's helper (`opengrep_source.py build --yes`, see the
`orbyn-opengrep` skill), then, from the repository root:

```sh
opengrep test --config .opengrep/rules/rust.yaml .opengrep/tests/rust.rs
opengrep test --config .opengrep/rules/web.yaml .opengrep/tests/web.ts
opengrep scan --config .opengrep/rules --exclude .opengrep --exclude node_modules --exclude generated .
```

Use only local rules: a `p/...` ruleset or `opengrep validate` downloads from `semgrep.dev`.

## Status against CodeQL (record updated 2026-11-27)

Gap 3 of issue #849 is closed for the literal replacement the rule matches: `orbyn-js-regex-escape-skips-backslash`
covers `js/incomplete-sanitization` written as `"\\$&"` or `'\\$&'` — the same replacement in either quote style — with
a matching case and a safe look-alike for each, and the one instance it named
(`web/src/styles/tokens.test.ts:165`) is fixed, so the helper now carries a backslash through instead of eating it.

What the rule does not read, so this record does not claim it: a replacement built another way (a variable, `$1` with a
capture group, `replaceAll`); a regex flag other than `g`, so `/[…]/gi` and a flagless `/[…]/` are not matched; a negated
class, which is skipped even where it really does exclude a backslash (`/[^\\]/` is not reported); and a class that
matches a backslash of its own, which needs no fix.

The other four gaps are **open** and need the owner's decision. None is accepted here, and the advisory workflow stays
non-required. The parser set below is the 2026-11-23 re-check; this change did not re-run the full scan.

- **Gap 1 — parser limits, not rules.** Opengrep v2.0.0 could not parse five files under `crates/orbyn-core/src`
  (`host/process.rs:557`, `host/process/unix_tree.rs:85`, `governance/computer/mod.rs:323`,
  `governance/computer/cdp.rs:560`, `crew/cancel.rs:156`) or `web/src/api/client.ts:262`; the issue also names
  `crates/orbynd/src/bin/orbyn_fake_engine/acp.rs` and `desktop/src-tauri/src/system.rs`. A clean scan says nothing
  about them. Decision needed: wait for a parser fix, rewrite the constructs, or keep CodeQL's Rust analysis for them.
- **Gap 2 — no rule for what CodeQL's Rust queries find.** `rust/path-injection`, `rust/sql-injection` and
  `rust/cleartext-logging` have taint rules with tests. Still missing: cleartext transmission (telling `http` from
  `https` needs the configured endpoint, which a rule cannot see) and the cross-file flow where request data reaches
  a filesystem path across the route/core boundary (needs `--enable-interfile` and `taint_interfile: true`, a workflow
  change neither made nor measured). Decision needed: accept the gap or fund those rules.
- **Gap 4 — the side-by-side run has not happened.** Decision needed: choose the stretch in advance, then record what
  each scanner reports and the other misses (the CI-gate method in the `orbyn-opengrep` skill).
- **Gap 5 — the session-storage question.** Ten test files write the launch token with
  `sessionStorage.setItem("orbyn.token", ...)` (the issue counted seven). Decision needed: whether the product should
  keep it there; it is a design decision, not a rule.

Do not turn CodeQL off until gap 4 has run for that stretch and the owner has accepted gaps 1, 2 and 5.

## Findings are leads

A finding is a place to look, not a verdict. The 29 SQL-format findings on that snapshot were read (18 in product
code, the rest in tests) and none builds SQL from request data: fixed table names, `?` placeholder lists and
constants sit next to bound parameters. The rule stays as an audit lead.
