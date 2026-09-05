# 1112 - Release Gate Diagnosability

Roadmap: [`../004-release-gate-diagnosability.md`](../004-release-gate-diagnosability.md)
Spec: [`../../../specs/119-release-gate-diagnosability-strict-lane.md`](../../../specs/119-release-gate-diagnosability-strict-lane.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md), [`../../../contracts/039-pre-release-ci-proof-contract.md`](../../../contracts/039-pre-release-ci-proof-contract.md)

Status: Complete
Owner: release gate runner, release text/JSON renders, release progress seam
Created: 2026-09-05
Ready since: 2026-09-05 operator direction
Completed: 2026-09-05; PR `90` merged at `f1732c87`

## Purpose

Leave enough on disk and on screen after a failed release gate that the
failure can be diagnosed without rerunning it.

## Work

- in the shared gate runner (`run_release_gate` and its progress wrapper in
  `crates/effigy-release/src/lib.rs`), write
  `.effigy/reports/release/gates/<gate-name>.log` per executed gate and
  `.effigy/reports/release/gates/environment.json` once per run, per spec
  `119` fixed decisions; create the directory as needed; latest run wins
- record `log_path` on each gate result and `environment_path` on the run
  report, and expose both as additive optional JSON fields
- in `append_gate_status_lines` (prepare/execute/status text), print the last
  20 lines of combined stdout/stderr plus the log path for each failed gate;
  add the log path to `render_release_gate_run_text`
- remove the terminal check in `release_progress_enabled`; progress always
  goes to stderr
- in `run_standalone_release_gates`, emit
  `configured gates (N): <names in order>` before the first gate starts
- redact captured environment values whose key contains `TOKEN`, `SECRET`,
  `KEY`, `PASSWORD`, or `CREDENTIAL`
- update guide `051` (what a gated run leaves behind, where to look after a
  failure) and guide `017` (new additive fields)
- append `CHANGELOG.md` `[Unreleased]` entries under **Added** and **Changed**
- close with one evidence log

## Acceptance

- [x] a fixture repo with a failing shell gate, run under captured stderr,
      leaves `<gate>.log` with full stdout and stderr and `environment.json`
- [x] a passing gate also leaves its log; passed gates render as one line
- [x] prepare text on gate failure shows the failing gate's tail (20 lines
      max) and its log path, and still reports `Prepared: no` with every
      mutation restored
- [x] `environment.json` records shell, cwd, `PATH`, `HOME`, `CARGO_*`,
      `RUSTUP_*`, `RUSTFLAGS`; a `CARGO_REGISTRY_TOKEN` set in the test
      environment appears as `<redacted>`
- [x] `release gates` under captured stderr emits the inventory line before
      any gate runs and one progress line per gate
- [x] `--json` for gates, status, prepare, and execute remain valid envelopes
      with unchanged schema ids; new fields are present and optional
- [x] no new flag, env var, gate kind, or non-rollback path

## Review Oracle

Falsify these counterexamples before PR creation:

1. A gated run leaves no log for an executed gate or no environment record.
2. A failed gate's text summary lacks the tail or the log path.
3. A token-like variable is written or rendered unredacted.
4. `release gates` is silent before the first gate under captured stderr.
5. Progress or inventory text reaches JSON stdout, or a schema id or existing
   field changes.
6. Gate order, fail-fast, `$SHELL -lc` invocation, or rollback changes.
7. Persistence lands outside `.effigy/reports/release/`.

## Validation

- focused tests in `crates/effigy-release/src/tests.rs` and the JSON-mode
  and help-render tests under `src/tests/` for every acceptance row
- `effigy graph affected` for changed source, then direct targets
- `effigy qa`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`
- do not use Effigy's own `ci` gate or a real release command as evidence

## Evidence Requirement

Write one dated closeout log under `docs/logs/2026-09/` mapping every oracle
row to exact proof: the fixture, the on-disk artifacts, the rendered tail, the
redaction check, the stderr inventory line, and validation output.

## Stop Conditions

Stop if the change needs a schema id bump, a new public flag, a
keep-on-failure or non-rollback path, a change to gate invocation or
ordering, a `.github/workflows/` edit, a release mutation, or persistence
outside `.effigy/reports/release/`.

## Next Task

PR `90` was opened at exact reviewed head `4a149cc3` and merged at `f1732c87`.
The coordinator notifies Chatterbox; Chatterbox tells the Swallowtail
Chatterbox the fix is on `main`.
