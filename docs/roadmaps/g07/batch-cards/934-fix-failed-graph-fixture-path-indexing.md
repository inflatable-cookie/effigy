# 934 - Fix Failed Graph Fixture Path Indexing

Roadmap: [`../016-failed-graph-fixture-path-reliability.md`](../016-failed-graph-fixture-path-reliability.md)
Strict lane: [`../../../specs/086-graph-follow-up-performance-and-fixture-reliability-strict-lane.md`](../../../specs/086-graph-follow-up-performance-and-fixture-reliability-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Remove or deliberately classify the seven known failed full-repo graph paths.

## Scope

- investigate the known fixture/bundle/export failures
- fix reusable structural support where appropriate
- add regression coverage for each fixed failure class
- rerun full-repo graph evidence

## Acceptance

- the known failed-path set is cleared or explicitly reclassified with proof
- full-repo graph indexing ends with `failed_paths = []`

## Results

- added template-aware TOML structural fallback for manifest-like files that
  include Jinja-style bundle templating
- kept template-rich bundle/export files indexable even when exact TOML parse
  and semantic manifest composition still fail
- skipped empty unresolved manifest edge targets instead of emitting invalid
  graph records for blank placeholder values
- added regression coverage for:
  - template-rich export manifests
  - template expressions with embedded quotes inside strings
  - blank unresolved targets in deploy/provider payloads
- full-repo `graph index --json` now reports `failed_paths = []`
- retained diagnostics are warnings only and document semantic compose fallback
  for template-heavy bundle/export surfaces

## Next Task

Execute `935`.
