# Northstar Refresh After g09 Closeout

Status: complete
Created: 2026-09-08
Roadmap: none — `g09` closed
Batch: northstar-refresh-2026-09-08

## Summary

- Verified Northstar Queue task `c541743f-067d-4ec1-a9ee-32ac12ab0140` is done,
  PR `95` merged, and closeout is published at
  `a701a938ad1d54865af2341e13358f403349c3c6`.
- Reviewed the instruction surface, docs spine, architecture and authority,
  planning readiness, triage, lifecycle, currentness, validation, and release
  boundary.
- Repaired stale front-door pointers that still advertised card `1117`, g09
  closeout, or this refresh as future work.
- Kept `g10` unopened and recorded the remaining lifecycle conflict in triage.

## Facet Results

| Facet | State | Evidence |
| --- | --- | --- |
| Instructions | current | Root `AGENTS.md` is project-specific; `CLAUDE.md` is the exact bridge. Installed Northstar audit passed with advisory size signals only. |
| Docs spine | repaired | Vision, contract, spec, roadmap, generation, and log front doors now agree that g09 is closed with no ready lane. |
| Architecture and authority | current | Architecture `000`, package map `010`, and active seams `023`, `024`, and `026` match the current workspace and shipped g09 boundaries. |
| Planning readiness | coherent, paused | Every g09 roadmap/card is complete; no active spec, ready card, or authorized g10 lane exists. |
| Triage | repaired | Three existing notes remain open with current next checks; one lifecycle note was added. |
| Lifecycle | ambiguous | Expanded roadmap history has an explicit retention disposition, but the closed log window and queue-retained handoff still need one normalization ruling. |
| Validation | current | Agent-instruction audit and `effigy qa:docs` passed; docs context returned a valid bounded result after refreshing 28 stale graph files. |
| Distribution | current | Workspace metadata remains version `0.12.1`; release gates and release mutation remain explicitly operator-gated. |

## Triage Dispositions

- `20260901-092640`: keep open; next check is downstream Bovine replacement
  evidence or the next strategic runway checkpoint.
- `20260905-092527`: keep open; persisted gate evidence shipped, but a later
  authorized release attempt must prove whether keep-on-failure is still needed.
- `20260906-224721`: keep open for an operator decision; do not reopen g09.
- `20260908-161956`: captured the lifecycle conflict; resolved by the later
  closed-generation normalization.

## Vision Target Delta

- Primary tags: `MAINT`, `OPERATE`, `CONTRACT`
- Movement: closed implementation generation with stale next pointers ->
  coherent planning pause with one named lifecycle normalization blocker
- Remaining gap: closed log-window and handoff-retention normalization; next
  strategic runway remains operator-owned

## Validation Performed

- `effigy skill run --path /Users/tom/.agents/skills/northstar northstar/check:agent-instructions`
  - passed; `CLAUDE.md` bridge correct, advisory context-size findings only
- `effigy --json docs context "current planning state after g09 closeout and next task" --max-sections 8`
  - returned `effigy.docs.context.v1`; graph auto-refreshed 28 files
- `effigy --json papercuts`
  - parsed cleanly; five open entries before this refresh repair
- `effigy qa:docs`
  - passed
- `cargo metadata --no-deps --format-version 1`
  - all workspace packages report `0.12.1`

## Risks

- Historical docs-graph diagnostics still name four missing historical
  relations. They do not affect current authority or link-check validity.
- The retained closed handoff and unarchived log window should not be mistaken
  for active execution authority.

## Next Task

Superseded by the
[closed-generation lifecycle normalization](./08-173644-closed-generation-lifecycle-normalization.md).
Do not open `g10` until the operator chooses the next strategic runway.
