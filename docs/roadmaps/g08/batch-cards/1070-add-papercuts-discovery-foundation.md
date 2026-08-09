# 1070 - Add Papercuts Discovery Foundation

Roadmap: [`../027-papercuts-discovery-and-capture.md`](../027-papercuts-discovery-and-capture.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md),
[`../../../contracts/036-papercuts-discovery-contract.md`](../../../contracts/036-papercuts-discovery-contract.md)

Status: Complete
Owner: Platform
Created: 2026-08-09
Ready after: operator selected `g08.027`

## Purpose

Provide deterministic read-only papercut discovery from either one project or
a directory containing sibling projects.

## Owner And Seam

`effigy-papercuts` owns scope discovery, Markdown parsing, diagnostics,
ordering, fingerprints, and report models. `effigy-cli` owns grammar/help. The
root runner owns cwd handoff and rendering.

## Work

- add the focused domain crate and fixtures
- parse canonical and multiline open/closed entries tolerantly
- discover one nearest project or immediate child project roots
- add the first-class default/list command, help, and human rendering
- emit `effigy.papercuts.v1` within the standard command envelope
- prove nested template exclusion and malformed-entry diagnostics

## Acceptance

- [x] project mode reads exactly one root queue
- [x] collection mode reads immediate child project-root queues only
- [x] open-only default and `--all` ordering are deterministic
- [x] multiline fields survive normalization
- [x] diagnostics identify source path and line
- [x] JSON and human output share one report model

## Validation

- `cargo test -p effigy-papercuts`
- focused CLI parser/help tests
- focused command-level text/JSON tests
- formatting and focused Clippy

## Stop Conditions

Stop if read-only discovery requires mutation, recursive unbounded traversal,
Northstar runtime coupling, or a command that cannot run without a repo root.

## Next Task

Card 1071 completed capture and lane closeout.
