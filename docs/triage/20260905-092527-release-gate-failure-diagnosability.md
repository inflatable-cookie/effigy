# Release Gate Failure Diagnosability

Status: open — intake reconciled, awaiting operator direction
Created: 2026-09-05
Owner: chatterbox
Source: Swallowtail Chatterbox handoff (operator-requested, 2026-09-05);
Swallowtail `PAPERCUTS.md` entries 2026-09-05 and 2026-09-02
Surfaces: `crates/effigy-release/src/lib.rs` (`run_release_gate`,
prepare gate path), `crates/effigy-release/src/text.rs`
(`append_gate_status_lines`), `src/runner/release_command/ops.rs`
(`release_progress_enabled`)
Related contracts: [`035`](../contracts/035-release-tag-identity-contract.md),
[`039`](../contracts/039-pre-release-ci-proof-contract.md);
guide [`051`](../guides/051-release-orchestration.md)

## Issue

When a configured gate fails during `release prepare --check-gates`, Effigy
rolls back the mutations and reports only `gate <name> failed` plus an exit
code. The gate's stdout/stderr are not shown in text mode and are not written
anywhere. Swallowtail lost two authorized candidate attempts (exit 101, 200s
and 47s) and spent ~2h reproducing by hand; v0.4.0 lost two more the same way.
The failure is environment-specific and still unnamed because nothing captured
it.

## Known (verified against `main` at `e543ef27`, same code as Swallowtail's `aafbd93`)

- Gate stdout/stderr **are captured** in memory (`GateResult`, lib.rs ~510).
  `release gates` text output and every `--json` render (status, gates,
  prepare, execute) already include them. Only the prepare/execute **text**
  renderer drops them: `append_gate_status_lines` prints exit code and
  duration only. So request 1 is a render gap plus a persistence gap, not a
  capture gap.
- Immediate workaround available today with no code change:
  `effigy --json release prepare --yes --check-gates --version X` retains the
  failing gate's full stdout/stderr in the rolled-back report.
- Nothing is persisted to disk by the release path itself. The
  `.effigy/reports/tasks/<gate>/latest.json` Swallowtail found is the task
  runner's report, written only because the gate command invokes an Effigy
  task.
- Gates run via `$SHELL -lc <command>` (login shell). Different agent shells
  therefore load different profiles and PATH. This is a concrete candidate
  for the "environment drift" hypothesis and is currently not recorded.
- `[release] running gate ...` progress lines exist but are emitted only when
  stderr is a terminal (`release_progress_enabled`). Under an agent or CI
  capture, `release gates` is silent until all gates finish. The configured
  gate inventory is never printed up front in either mode.
- Rollback on gate failure is deliberate: the code comment says a mutated
  tree must not survive behind a `Prepared: no` report. A keep-on-failure
  flag changes that invariant and must define what the state file and
  `release status` say about a kept-but-unprepared tree.

## Unknown

- Whether the operator wants this as one bounded Effigy lane now, ahead of the
  open consumer-maturity checkpoint, or queued behind it.
- Whether persisted gate logs belong under `.effigy/reports/release/` (new)
  or beside the existing task reports; and retention policy (latest only vs
  per-attempt).
- Redaction rules for environment capture (which variables are sensitive).
- Whether keep-on-failure is wanted at all once logs and env capture exist;
  Swallowtail ranks it third.

## Requested changes (source priority order, tentative)

1. Persist each gate's stdout/stderr to a log file during prepare and
   execute; print the tail of the failing gate in the text rollback summary.
2. Record gate execution environment (shell, cwd, PATH, `CARGO_*`,
   `RUSTUP_*`, `RUSTFLAGS`, `HOME`, redacted) at gated-run start.
3. `--keep-on-failure` for `release prepare` leaving mutations plus a marker.
4. `release gates` prints the configured gate names immediately and emits
   progress when stderr is not a terminal.

## Chatterbox assessment (not operator-confirmed)

Items 1, 2, and 4 are small, additive, and touch only the release crate's
render, a new report writer, and the progress gate. They fit one bounded
`g09` papercut-class lane with focused tests. Item 3 is a semantic change to
the prepare transaction and should be a separate decision, not bundled.
Effigy's own releases share the same blind spot, so this is not consumer-only
value.

## Next Task

Operator decides: dispatch a bounded diagnosability lane (items 1, 2, 4), and
whether to sequence it before or after the consumer-maturity checkpoint in
[`20260904-130553`](./20260904-130553-consumer-maturity-scoring-checkpoint.md).
On confirmation, chatterbox promotes roadmap, spec, card, and manifest, and
prunes this note. Reply the `--json` workaround to Swallowtail regardless.
