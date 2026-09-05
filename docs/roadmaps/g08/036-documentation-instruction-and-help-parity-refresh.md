# g08.036 - Documentation, Instruction, And Help Parity Refresh

Status: Complete
Depends on: current `main` and completed `g08.034`
Spec: [`109`](../../specs/archive/109-documentation-instruction-and-help-parity-refresh.md)

## Goal

Make current Effigy behavior discoverable and consistent across active project
documentation, agent instructions, generated reference output, and shipped CLI
help, with scan evidence and proportional recurrence guards.

## Vision Alignment

- Primary tags: `CONTRACT`, `OPERATE`, `MAINT`, `ROUTE`
- Target envelope: users and agents can enter through docs, instructions, or
  CLI help and reach the same current behavior without reconstructing it from
  source.
- Vision target delta: refresh the completed parity baseline after subsequent
  feature work and make the instruction/help boundary part of the evidence.

## Execution Plan

- [x] card `1091`: run the scan, AGENTS, feature coverage, active docs, built-in
      help, generated reference, repair, recurrence, validation, and closeout
      loop as one coherent maintenance batch

## Owner And Seam

Live parsers, descriptors, registries, manifest types, schemas, and behavior
tests remain implementation-side truth. Active docs and instruction surfaces
own explanation and routing. Help/config renderers and their tests may change
to restore truthful discovery; runtime meaning may not change in this lane.

## Non-Goals

- no command, runtime, manifest, JSON, persistence, or container behavior change
- no code-quality refactor solely to clear a scan finding
- no release, dependency, or CI workflow mutation
- no rewrite of logs, archived specs, or closed planning evidence
- no broad prose restyling without a verified coverage or currentness gap

## Acceptance

- [ ] strict spec `109` acceptance is satisfied with evidence
- [ ] every in-scope gap is repaired or blocked with a concrete reason
- [ ] full scan and feature-to-doc/help matrices are published
- [ ] Northstar AGENTS review and bounded repairs are recorded
- [ ] required focused and full validation pass
- [ ] card, roadmap, spec, log, and front doors return one honest next task

## Evidence

- Planning: [`2026-08/30-164636-documentation-instruction-help-refresh-planning.md`](../../logs/archive/2026-08/30-164636-documentation-instruction-help-refresh-planning.md)
- Closeout: [`30-174452-documentation-instruction-help-parity-closeout.md`](../../logs/archive/2026-08/30-174452-documentation-instruction-help-parity-closeout.md)

## Next Task

Continue the active documentation graph lane at ready card
[`1090`](./batch-cards/1090-prove-generic-and-northstar-profiles.md).
Card `1089` closed on 2026-08-31.
