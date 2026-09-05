# 119 Release Gate Diagnosability Strict Lane

Status: Complete
Owner: Effigy orchestrator
Created: 2026-09-05
Roadmap: [`g09.004`](../roadmaps/g09/004-release-gate-diagnosability.md)
Ready card: [`1112`](../roadmaps/g09/batch-cards/1112-release-gate-diagnosability.md)
Contracts: [`035`](../contracts/035-release-tag-identity-contract.md),
[`039`](../contracts/039-pre-release-ci-proof-contract.md)
Guides: [`051`](../guides/051-release-orchestration.md),
[`017`](../guides/017-json-output-contracts.md)
Source: consumer intake from Swallowtail (2026-09-05); two authorized
`release prepare --check-gates` attempts failed the `floor` gate with only
`gate floor failed` and an exit code retained.

## Outcome

A failed release gate names itself. Every gated run leaves the gate's full
output and the execution environment on disk, the text rollback summary shows
the failing gate's tail, and `release gates` announces its inventory and
progress whether or not stderr is a terminal.

## Fixed Decisions

- Gate stdout/stderr are already captured in memory and already present in
  every `--json` release render. This lane adds persistence and text
  visibility; it does not change how gates run or what they capture.
- Persistence lives under `.effigy/reports/release/gates/`, written by the
  shared gate runner so every caller (`release gates`, `status --check-gates`,
  `prepare --check-gates`, and `execute` gate reruns) gets it without
  per-command wiring:
  - `<gate-name>.log` per executed gate: command, cwd, started-at, exit code,
    duration, then full stdout and stderr in labelled sections. Latest run
    wins, matching the existing `latest.json` task-report convention.
  - `environment.json` once per gated run: resolved shell, cwd, `PATH`,
    `HOME`, and every `CARGO_*`, `RUSTUP_*`, and `RUSTFLAGS` variable. Any
    captured key whose name contains `TOKEN`, `SECRET`, `KEY`, `PASSWORD`, or
    `CREDENTIAL` is recorded with the value `<redacted>`.
- Text renders for prepare and execute show, for each failed gate, the last
  20 lines of combined stdout/stderr and the log path. `release gates` text
  keeps its full output and adds the log path. Passed gates stay one line.
- JSON renders gain additive optional fields only: `log_path` per gate result
  and an `environment_path` for the run. Existing schema ids
  (`effigy.release.gates.v1`, `effigy.release.prepare.v1`,
  `effigy.release.execute.v1`, `effigy.release.status.v1`) are unchanged.
- Release progress lines always go to stderr; the terminal check is removed.
  `release gates` emits the configured inventory (`configured gates (N):
  a, b, c`) before the first gate starts. JSON stdout stays clean.
- The prepare rollback invariant is unchanged: a failed gate still restores
  every mutation. A keep-on-failure mode is a separate decision and is out of
  scope.
- No new CLI flag, environment variable, or gate kind is introduced.

## Dependency Runway

```text
Swallowtail intake + operator direction (2026-09-05)
  -> 1112 persist gate output/environment, surface failing tail, announce inventory
  -> exact-head review and merge
  -> Swallowtail adopts through the normal local-install route (outside this lane)
```

One worker owns card `1112`. The change is additive and bounded to the
release crate and its CLI progress seam, so use an economical non-frontier
day-to-day worker. Material review remains with the orchestrator.

## Whole-Lane Review Oracle

Reject the lane if any counterexample survives:

1. A gated run finishes without `<gate-name>.log` for every executed gate or
   without `environment.json`.
2. A failed gate's log lacks its full stdout or stderr, or the text prepare
   summary shows only `gate <name> failed` with no tail or log path.
3. A `CARGO_REGISTRY_TOKEN`-style variable appears unredacted in
   `environment.json` or in any rendered output.
4. `release gates` produces no stderr line naming the configured gates before
   the first gate runs when stderr is not a terminal.
5. JSON stdout for any release command contains a progress or inventory
   line, or an existing schema id or field is renamed or removed.
6. Gate execution semantics change: order, fail-fast, shell invocation,
   rollback on failure, or the `Prepared: no` invariant.
7. A new flag, environment variable, workflow edit, or release execution
   appears.

Smallest counterexample set: one failing shell gate under a captured-stderr
harness; one passing gate; one gated run with `CARGO_REGISTRY_TOKEN` set; one
JSON prepare against a fixture repo with a failing gate; one `release gates`
run against a fixture with two gates.

## Validation And Evidence

Card `1112` maps every oracle row to named proof. Prove behavior with fixture
repositories in the release crate and JSON-mode tests; do not run Effigy's
own `ci` gate as evidence. Run `effigy qa`, `cargo fmt --all -- --check`,
`cargo clippy --all-targets -- -D warnings`, and `git diff --check`. Write one
dated evidence log.

## Stop Conditions

Stop and return to the orchestrator if the change needs a schema id bump, a
new public flag, a keep-on-failure or non-rollback path, a change to gate
invocation or ordering, a `.github/workflows/` edit, a release mutation, or
persistence outside `.effigy/reports/release/`.

## Next Task

Card `1112` complete. PR `90` merged at `f1732c87`; evidence is recorded in
the dated closeout log.
