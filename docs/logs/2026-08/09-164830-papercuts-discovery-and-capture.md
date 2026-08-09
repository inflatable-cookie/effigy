# Papercuts Discovery And Capture

Status: complete
Created: 2026-08-09
Roadmap: g08.027
Batch: papercuts-discovery-and-capture

## Summary

- Added first-class project and sibling-project `PAPERCUTS.md` discovery.
- Added deterministic human output and `effigy.papercuts.v1` JSON within the
  standard command envelope.
- Added safe single-project capture with required fields, duplicate-open-title
  rejection, locking, and atomic replacement.
- Published the command, JSON, agent, and Northstar boundary guidance.
- Closed cards `1070`, `1071`, strict spec `100`, and roadmap `g08.027`.

## Portfolio Proof

`effigy --json papercuts --scope /Users/tom/Dev/projects` scanned 25 immediate
project roots, found nine root queues and 25 open entries, excluded nested
template queues, and returned zero diagnostics. Closed legacy entries remain
available through `--all` without creating actionable missing-field noise.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Movement: baseline `papercut observations isolated in individual project
  Markdown files` -> current `one deterministic portfolio inventory with safe
  local capture and a stable agent contract`
- Remaining gap: None. Scheduling and prioritization remain external operator
  or agent policy.

## Validation Performed

- `effigy qa:ci:fast`
  - result: 1,637 passed, one skipped; full JSON contracts passed
- `effigy qa:docs`
  - result: pass
- `cargo clippy --all-targets -- -D warnings`
  - result: pass
- `cargo test -p effigy-papercuts`
  - result: five passed
- focused parser, runner, and CLI output tests
  - result: seven passed
- real portfolio JSON discovery
  - result: 25 projects, nine queues, 25 open entries, zero diagnostics

## Boundaries

No recursive arbitrary-directory scan, semantic prioritization, issue or
roadmap creation, papercut close operation, workflow edit, or release mutation
was introduced.

## Next Task

Run `effigy --json papercuts --scope ~/Dev/projects` from the periodic agent and
select any resolution lane explicitly.
