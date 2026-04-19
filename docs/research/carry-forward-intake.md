# Research Carry-Forward Intake

Purpose: keep future-facing research residue visible without pretending it is a
live `g02` product roadmap or a `v0.3` blocker.

## Why This Exists

The old `g02.018` roadmap mixed three different kinds of work:

- research-corpus promotion and indexing
- future-facing research questions
- implied downstream planning and product work

That is the wrong control surface for release prep.

The research phases are already complete. The remaining questions should enter
the normal research intake flow, then graduate into later planning only when
they are evidence-backed, prioritized, and ready to shape `g03`.

## Intake Rule

Use this file as the intake register for research carry-forward that is:

- non-blocking for `v0.3`
- too research-shaped to live in the active `g02` roadmap queue
- likely to feed future planning, not immediate release work

Promotion path:

1. intake here
2. research batch against primary sources
3. promotion into maintained docs or future roadmap planning only when the
   conclusion is specific enough to constrain design

## Current Intake

### Research Corpus Promotion

- reconcile `g01.020`–`g01.022` against shipped code and maintained docs
- tighten the research index and cross-reference surfaces where shipped
  conclusions are still under-promoted
- capture any remaining promotion-only cleanup as bounded documentation work,
  not as a broad product roadmap

### Developer Experience Residue

- completion UX residue that still warrants fresh evidence after the shipped
  CLI/help surface
- cross-platform portability follow-up that actually survives the current
  shell/runtime cleanup
- any durable DX pattern-library material that proves worth maintaining after
  renewed research, not by assumption

### Scale And Integration Residue

- remote execution strategy
- IDE/editor integration posture
- plugin/extensibility posture
- telemetry and observability posture

These are explicitly future-facing. They require fresh research, then planning,
then later product work if promoted.

## Release Posture

None of the items in this intake block `v0.3`.

They are inputs to future research and `g03` shaping, not to the current
release-prep queue.

## Next Task

Keep release prep on `g02.007`.

Resume this intake only through normal research batching after `v0.3`, or
earlier if one item becomes an explicitly approved research thread.
