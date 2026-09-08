# Closed Generation Lifecycle Drift

Status: open — normalization decision required
Created: 2026-09-08
Owner: chatterbox
Source: Northstar Refresh after `g09` closeout
Contract: [`001`](../contracts/001-working-rules.md)
Roadmap: [`generation index`](../roadmaps/generation-index.md)

## Issue

`g09` is closed, but two lifecycle surfaces do not yet agree with the repo's
maintenance rules:

- `docs/logs/2026-09/` remains the active log window even though the retention
  convention says a closed generation's month window moves under
  `docs/logs/archive/`.
- Queue closeout initially deleted the worker handoff as contract `001`
  requires. Northstar Queue rejected that publication because its verifier
  requires every changed Markdown path to remain a regular file, so corrective
  commit `a701a938ad1d54865af2341e13358f403349c3c6` restored it as a closed,
  non-dispatching record.

## Known

- All nine `g09` roadmaps and cards are complete. Spec `124` is archived and no
  execution lane is ready.
- The retained handoff has `status: closed`; it cannot authorize another worker.
- Roadmap history has an explicit repo-local disposition to remain expanded.
  Compaction targets front doors and logs, not generation directories.
- Northstar Queue task `c541743f-067d-4ec1-a9ee-32ac12ab0140` is done.

## Unknown

- Whether Effigy should revise contract `001` to permit queue-retained closed
  records, archive such records under a dedicated handoff archive, or require
  the plugin to accept canonical deletion.
- Whether the `2026-09` log window should move immediately or stay visible until
  the next generation is selected.

## Next Task

Use Northstar docs normalization to reconcile the closed log window and handoff
retention rule as one bounded lifecycle batch. Preserve the closed task record
until its destination is explicit. Do not open `g10` or execute release work as
part of that normalization.
