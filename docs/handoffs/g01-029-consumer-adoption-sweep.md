## What This Thread Was Doing

Closing `g01.029` after the Northstar + Effigy consumer-adoption kit had
already reached a trustworthy product boundary inside the Effigy repo.

The remaining ask is not more Effigy implementation. It is one external sweep:
check a small real repo cohort and confirm the current skill/templates/docs are
still enough without reopening product scope by habit.

## Why It Matters

`g01.029` should not stay open just because the ecosystem is large. The Effigy
side is already in good shape. But one final sweep gives a cleaner answer on
whether any real adoption pain still deserves product work or whether the
remaining effort belongs entirely in skill/template maintenance.

## Current State

- Effigy-side consumer adoption work is treated as complete.
- The canonical roadmap file is:
  `/Users/tom/Dev/projects/effigy/docs/roadmaps/g01/029-northstar-effigy-consumer-adoption-kit.md`
- The evidence chain is already in that roadmap under `Validation Evidence`.
- The active release posture is deferred until core blockers are resolved; this
  sweep is not a release blocker.

## Boundaries

- Do not touch `src/**` or `crates/**`.
- Do not reopen product implementation scope unless the sweep shows repeated,
  concrete adoption pain.
- Do not change the active strict lane.
- Keep the sweep focused on consumer adoption evidence, not a fresh doctrine
  rewrite.

## Important Context

What should be tested in the sweep:

- can a consumer repo still adopt the current Northstar + Effigy contract with
  the existing skill/template/docs stack
- are `AGENTS.md`, docs skeleton, `qa:northstar`, and docs-policy guidance
  still coherent outside the Effigy repo
- is there repeated friction that clearly belongs in product code rather than
  skill/template/docs maintenance

Good evidence sources:

- the existing cohort linked from the roadmap
- one or two fresh consumer repos if they are materially different
- real friction notes, not hypothetical polish ideas

## Suggested Next Move

Run a short consumer-repo sweep and produce one closeout note that answers:

1. what still works cleanly
2. what friction was found
3. whether any friction is repetitive enough to justify reopening Effigy-side
   product scope

Default outcome should be `keep 029 closed` unless the evidence is strong.

## Completion Protocol

1. Write one concise sweep log in `/Users/tom/Dev/projects/effigy/docs/logs/`.
2. If the sweep finds no repeated product-level pain, leave `g01.029` closed.
3. If the sweep finds repeated product-level pain, open a fresh `g02` roadmap
   instead of silently reactivating `g01.029`.
4. Update `/Users/tom/Dev/projects/effigy/docs/logs/README.md` if you add a
   new log.
