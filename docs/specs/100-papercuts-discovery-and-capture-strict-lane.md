# 100 Papercuts Discovery And Capture Strict Lane

Roadmap: [`g08.027`](../roadmaps/g08/027-papercuts-discovery-and-capture.md)
Durable authority:

- [`contract/036`](../contracts/036-papercuts-discovery-contract.md)
- [`architecture/010`](../architecture/010-package-map.md)
- [`working rules/001`](../contracts/001-working-rules.md)

Status: Complete
Owner: Platform
Created: 2026-08-09

## Purpose

Let operators and periodic agents discover actionable papercuts across sibling
projects, while retaining deterministic single-project behavior and the
observation-not-backlog boundary.

## Lane Posture

Posture: `strict-complete`

Current ready card: none

## Owner And Seam

`effigy-papercuts` owns Markdown parsing, project/collection discovery,
normalized reports, diagnostics, fingerprints, and safe queue insertion.
`effigy-cli` owns grammar/help only. The root runner supplies invocation cwd,
dispatch, and presentation. Northstar remains the external producer-contract
owner; there is no runtime dependency on Northstar.

## Ready Chain

1. `1070` adds the read-only domain, command grammar, human output, JSON, and
   project/collection proof.
2. `1071` adds safe capture, documentation, full QA, and lane closeout.

## Acceptance

- [x] `effigy papercuts` works from one project and a sibling-project directory
- [x] only project-root queues are read; nested templates are excluded
- [x] multiline entries and malformed-entry diagnostics are deterministic
- [x] `effigy --json papercuts` emits `effigy.papercuts.v1`
- [x] `papercuts add` creates or preserves a canonical queue safely
- [x] no prioritization, issue creation, promotion, close, or roadmap mutation
      behavior is introduced

## Stop Conditions

Stop and replan if discovery needs recursive arbitrary filesystem traversal,
semantic/LLM deduplication, a runtime Northstar dependency, a workflow edit,
or any release mutation.

## Next Task

Use the machine-readable portfolio command for periodic triage. Select any
follow-up implementation lane explicitly.
