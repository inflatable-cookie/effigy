# 1101 - Bound Docs Context Cold Refresh

Roadmap: [`../046-docs-context-time-budget-papercut.md`](../046-docs-context-time-budget-papercut.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md), [`../../../contracts/041-documentation-graph-profile-contract.md`](../../../contracts/041-documentation-graph-profile-contract.md)
Papercut: [`PAPERCUTS.md`](../../../../PAPERCUTS.md)

Status: Complete
Owner: graph command time-budget boundary
Created: 2026-09-01
Ready since: 2026-09-01 operator-approved papercut routing
Completed: 2026-09-01
Evidence: [`../../../logs/2026-09/01-184159-docs-context-time-budget-1101.md`](../../../logs/2026-09/01-184159-docs-context-time-budget-1101.md)

## Purpose

Apply the graph command's existing wall-clock policy and typed timeout evidence
to lazy graph refresh triggered by `effigy docs context`.

## Work

- extract or expose one shared graph time-budget/bounded-operation owner
- route docs-context lazy refresh through it without a second refresh path
- emit a concise cold/stale refresh progress notice on stderr
- prove text, JSON, disabled-bound, warm, and graph-command parity
- close this card with one evidence log

## Acceptance

- [x] a forced slow cold refresh with a tiny `EFFIGY_GRAPH_TIMEOUT_MS` exits
      within the configured bound
- [x] timeout detail uses the existing typed graph-timeout schema, command
      identity, health snapshot, and direct recovery guidance
- [x] text and JSON share the bound; JSON stdout remains a valid standard envelope
- [x] cold/stale refresh emits progress before the walk; warm/current query does not
- [x] `EFFIGY_GRAPH_TIMEOUT_MS=0` disables the bound for both graph and docs paths
- [x] no second index, refresh path, timeout parser, or background service appears

## Review Oracle

Falsify these counterexamples before PR creation:

1. Docs context still blocks beyond a deliberately tiny bound on a cold graph.
2. JSON progress contaminates stdout or makes the command envelope invalid.
3. Timeout lacks the shared health snapshot or advertises different recovery.
4. Warm/current queries emit a false refresh-progress message.
5. `0` differs between graph and docs consumers.
6. Graph command timeout behavior or schema changes incidentally.

## Validation

- focused graph/docs command timeout, stderr, text, and JSON tests
- existing graph command timeout regression tests
- `effigy graph affected` for changed source, then direct targets
- `effigy qa`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`

## Evidence Requirement

Write one dated closeout log mapping every oracle row to exact proof. Record the
applied bound, measured bounded result, stderr/stdout separation, shared schema,
warm behavior, disabled-bound result, and validation.

## Stop Conditions

Stop if the fix requires a daemon, second graph store/refresh implementation,
new public timeout flag or schema, cancellation guarantees absent from the
existing graph timeout model, or documentation ranking changes.

## Next Task

Return the exact-head PR to the Effigy orchestrator.
