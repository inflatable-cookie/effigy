# Parallel Papercuts Frontier Planning

Date: 2026-09-01
Status: Ready for dispatch
Cards: `1100`, `1101`, `1102`

## Decision

The operator promoted three bounded Effigy-owned papercuts together. They are
safe to implement in parallel because their runtime write sets are disjoint:

- card `1100`: test suite task-reference resolver and focused recurrence tests
- card `1101`: root runner graph/docs command time-budget boundary
- card `1102`: `effigy-codegraph` docs-context selection and focused tests

No global worker budget applies. All three use economical day-to-day profiles.
None meets the exceptional reasoning and material-consequence threshold for a
frontier implementation worker.

## Serial Edges

- This planning and handoff commit precedes all worker branches.
- Workers do not edit shared front doors, `PAPERCUTS.md`, `CHANGELOG.md`,
  contracts `038`/`041`, or guide `079`; the orchestrator integrates those.
- Same-repository PRs merge one at a time after exact-head review and green gates.
- Acowtancy's workaround is downstream-owned and remains until revalidation.

Card `1099` stays ready in its existing strict lane. The existing catalog-pack
publication planning delegate keeps sole ownership of its triage packet. No
worker in this frontier duplicates either lane.

## Readiness

Each card names bounded acceptance, validation, evidence, stop conditions, and
a six-row review oracle. No manifest grammar, release/workflow, Acowtancy,
catalog publication, or broad roadmap change is authorized.

## Next Task

Commit and push one worker handoff per card, then launch all three workspaces.
