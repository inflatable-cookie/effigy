# Handoff: Northstar + Effigy Productization Follow-Through

Status: active
Created: 2026-03-12
Roadmap: g01.029

## Objective

Carry the Northstar + Effigy productization work from aligned source-of-truth docs into a fresh installed-skill workflow and identify the next real product gap from live use.

## Scope

- Verify the installed `northstar-effigy` and `northstar-handoff` skills from a fresh-agent perspective.
- Use those installed skills to drive one real continuation batch instead of relying on repo-local memory.
- Decide whether the next step is commit packaging, more dogfooding, or a concrete Effigy product surface gap.
- Do not reopen the broad consuming-repo migration sweep unless fresh use reveals a specific unresolved gap.

## Inputs

- `~/Dev/projects/effigy/docs/roadmaps/g01/029-northstar-effigy-consumer-adoption-kit.md`
- `~/Dev/projects/effigy/docs/logs/archive/2026-03/12-235900-source-of-truth-consolidation.md`
- `~/Dev/projects/effigy/docs/guides/056-northstar-effigy-consumer-repo-contract.md`
- `~/Dev/projects/northstar/skills/northstar-effigy/SKILL.md`
- `~/Dev/projects/northstar/skills/northstar-handoff/SKILL.md`
- `~/.codex/skills/northstar-effigy/SKILL.md`
- `~/.codex/skills/northstar-handoff/SKILL.md`

## Constraints

- Follow the repo instructions in `~/Dev/projects/effigy/AGENTS.md`.
- Keep the product boundary intact: the skill layer owns bootstrap/scaffolding, Effigy owns reusable validation/runtime/release surfaces.
- Do not reintroduce current-directory `--repo .` teaching in docs, skills, or examples.
- Keep work in meaningful batches and leave one clear next task at the end.

## Deliverables

- `~/Dev/projects/effigy/docs/logs/archive/2026-03/12-235950-northstar-effigy-productization-handoff.md`
- `~/Dev/projects/effigy/docs/logs/README.md`
- any touched source-of-truth docs or skill files needed to resolve gaps found during installed-skill dogfooding

## Acceptance Criteria

- The next thread can start from the installed Northstar skills and the listed source files without needing this conversation for missing context.
- The next thread leaves an explicit decision on the next productization move: commit boundary, further dogfooding, or a concrete Effigy feature gap.
- Any newly discovered friction is classified as either skill/template cleanup or an Effigy product gap, not left as vague follow-up.

## Notes

- Current context: the consumer-adoption sweep is effectively complete, `g01.029` has moved from migration into consolidation/product-boundary work, and the source-of-truth docs in both Effigy and Northstar now describe the same contract.
- Decisions: `effigy docs check-paths` was productized because it is clearly generic; bootstrap/init scaffolding was deliberately not productized and remains in the `northstar-effigy` skill. A first-class `northstar-handoff` skill now exists in Northstar and has replaced the legacy installed `handoff-contract` skill in `~/.codex/skills`.
- Watch-outs: the current shell session has accumulated many open unified exec processes, so the next thread should reuse sessions or keep shell usage tighter than this one did. Also distinguish historical log evidence from active docs before “cleaning up” older references.
- Next move: start a fresh thread using the installed `northstar-effigy` and `northstar-handoff` skills, treat this file as the execution brief, and use that run to decide whether the next concrete batch should be commit packaging or one more installed-skill dogfood pass on a calm target repo.

## Completion Protocol

1. Update the relevant roadmap or log surface with the outcome of the next batch.
2. Record unresolved blockers or risks explicitly.
3. Leave one clear next task for the following thread.
