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
  - `orbyn-js-regex-escape-skips-backslash` for `js/incomplete-sanitization`: `replace(/[…]/g, "\\$&")` whose
    character class does not escape a backslash (`web/src/styles/tokens.test.ts:165` is the known case).
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

## Status against CodeQL (re-checked 2026-11-23)

Four of the five query classes named in issue #849 now have a rule with tests: `rust/path-injection`,
`rust/sql-injection`, `rust/cleartext-logging` and `js/incomplete-sanitization`. What is still open before CodeQL can be
turned off:

- **Parser limits, not rules.** Opengrep v2.0.0 still cannot parse five files under `crates/orbyn-core/src`
  (`host/process.rs:557`, `host/process/unix_tree.rs:85`, `governance/computer/mod.rs:323`,
  `governance/computer/cdp.rs:560`, `crew/cancel.rs:156`) or `web/src/api/client.ts:262`. A clean scan says nothing
  about those files. The 2026-11-21 snapshot named six; `cdp.rs` and `client.ts` joined them since.
- **No interfile taint.** Request data reaches a filesystem path across the route/core boundary, where an intrafile rule
  cannot follow it. Closing that needs `--enable-interfile` in the scan and `taint_interfile: true` on the rule: a
  workflow change that was not made or measured with the runners disabled.
- **Cleartext transmission has no rule.** Telling `http` from `https` needs the configured endpoint, which a rule
  cannot see; a rule that fires on every authenticated call would be worse than none.
- **The side-by-side run has not happened** (the CI-gate method in the `orbyn-opengrep` skill).
- The session-storage question in the issue is a design decision, not a rule.

Do not turn CodeQL off until that side-by-side comparison has been run for a stretch chosen in advance.

## Findings are leads

A finding is a place to look, not a verdict. The 29 SQL-format findings on that snapshot were read (18 in product
code, the rest in tests) and none builds SQL from request data: fixed table names, `?` placeholder lists and
constants sit next to bound parameters. The rule stays as an audit lead.
