# Opengrep rules

Starter static-analysis rules for this project, run by the **advisory** workflow
[opengrep.yml](../.github/workflows/opengrep.yml). It is not a required check, never fails a pull request on a
finding, and **does not replace CodeQL**.

## What is here

- `rules/rust.yaml`, `rules/web.yaml`: six Rust and six TypeScript or JavaScript rules, written from the project's
  risk list (process spawning and shells, SQL built with `format!`, TLS verification, `unsafe`, secret-named values in
  logs, `eval`, unsafe HTML, secrets in browser storage, reflected HTML from a request URL, first-occurrence-only
  `replace`, incomplete HTML-comment stripping).
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

## Status against CodeQL (one snapshot, 2026-11-21)

On the same commit these rules found 7 of CodeQL's 11 open alerts. The other four are three Rust alerts that are
false positives on reading (a method named `conflicts`, a JSON field named `trusted`, a fixture string named
`secret_id`) and one regex-escaping case with no rule. The engine could not parse six Rust files (`&raw const`,
async closures), among them process and sandbox code. Nothing here covers what CodeQL's Rust queries find in product
code. Do not turn CodeQL off until the comparison on the `orbyn-opengrep` skill's CI-gate page has been run side by
side for a stretch chosen in advance.

## Findings are leads

A finding is a place to look, not a verdict. The 29 SQL-format findings on that snapshot were read (18 in product
code, the rest in tests) and none builds SQL from request data: fixed table names, `?` placeholder lists and
constants sit next to bound parameters. The rule stays as an audit lead.
