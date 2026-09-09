# Consumer Adoption Cohort Expansion

Status: open — unscheduled candidate
Created: 2026-09-09
Owner: Platform + Repository Maintainers
Source: retired roadmap backlog `docs/roadmaps/backlog/g09-candidate-themes.md`
  (Theme 3, 2026-08-17 through 2026-09-05); preserved here on backlog
  retirement without change of meaning
Vision: [`007-vision-adoption-and-maturity-model-v1.md`](../vision/007-vision-adoption-and-maturity-model-v1.md)
  (section 6 adoption posture) and
  [`020-strategic-runway-atlas-v1.md`](../vision/020-strategic-runway-atlas-v1.md)

## Purpose

Preserve the one surviving candidate from the `g09` theme backlog: expanding
the consumer adoption cohort beyond the completed pilot. Theme 1 completed as
`g08.032`, Theme 2 completed as a narrow `g08` extension, Theme 4 completed as
the `g09.001` preview and was rolled back by `g09.002`, and Theme 3's pilot
completed as `g09.003` (card `1111`, PR `88`). Theme 5 release execution is
not a candidate task: current release guides, contracts, and operator gates
already own it.

This note is non-authoritative. Do not treat it as an execution queue.

## Scope

Expand the replay cohort only when a consumer asks or retrieval evidence
motivates it:

- at least one non-fixture consumer repo passes `qa:northstar` end-to-end
  with archived evidence log and contract drift notes fed back into starter
  templates.

## Constraints

- The maturity question is settled: consumers are tracked on the vision `007`
  section 6 adoption posture (pass, fail, unavailable, n/a per check;
  `aligned`, `drifted`, or `unobserved` overall), never on stage numbers.
- Do not force the workspace-container shape onto single-repo pilots.
- Direct invocation remains canonical.
- Do not reopen the completed `v0.5.0` backlog or mix this candidate with
  unrelated feature work.

## Open Questions

- Which consumer, if any, will volunteer the next replay window?
- Does `docs context --sources` evidence motivate a cohort replay before a
  consumer asks?

## Promotion Conditions

Primary tags: `RELEASE`, `OPERATE`, `CONTRACT`.

Target envelope: at least one non-fixture consumer repo passes
`qa:northstar` end-to-end with archived evidence log and contract drift notes
fed back into starter templates.

Promotion signals:

- starter `AGENTS.md` / `effigy.toml` / docs-policy parity with guide `056`;
- cohort log linked from the `g01` milestone record or successor milestone;
- an active execution window exists with operator intent behind it.

Promotion requires operator intent, current canonical refs, an active
generation, and a ready top-level Northstar task.

## Next Task

Theme 3's maturity question is settled; cohort expansion is unscheduled until
a consumer asks or `docs context --sources` evidence motivates it. Next check:
the first consumer replay request, or the next operator-led strategic runway
checkpoint, whichever comes first. Do not promote this note into a task
without that direction.
