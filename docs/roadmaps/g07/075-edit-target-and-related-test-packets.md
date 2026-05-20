# g07.075 - Edit Target And Related Test Packets

Status: Complete
Depends on: `g07.074`

## Goal

Make graph packets answer the next practical agent question: "what file should
I edit, and what should I test?"

The graph already returns owners, excerpts, related symbols, and affected
flows. This lane tightens the packet so agents need fewer follow-up file
opens and fewer speculative searches before editing.

## Problem

For split features, `graph explore` can land near the right area without
clearly separating:

- implementation owner
- adapter/wiring owner
- test owner
- docs/supporting owner

The init wizard inventory query showed this: the packet named `wizard.rs`,
`init.rs`, and `init/inventory.rs`, but did not make the ownership split
obvious enough for direct editing.

## Scope

- inspect current explore packet fields and related-symbol assembly
- add or refine labels for likely edit targets, likely tests, and supporting
  files
- prefer exact implementation symbols when available
- preserve excerpts and line ranges so agents can verify without opening every
  file
- make the packet useful across Rust, PHP, JS/TS, Python, Markdown, and TOML
  as far as the existing extractors support them

## Guardrails

- no false certainty; use confidence labels when ownership is inferred
- do not claim exhaustive tests
- do not require language support beyond the current first-party extractors
- do not make the packet huge
- do not remove lower-level graph commands

## Acceptance Criteria

- split-owner queries identify implementation and wiring roles separately
- likely tests are surfaced when naming or graph edges support them
- unclear cases report low confidence rather than pretending certainty
- JSON contract changes are documented and tested

## Next Task

Execute `1025`.
