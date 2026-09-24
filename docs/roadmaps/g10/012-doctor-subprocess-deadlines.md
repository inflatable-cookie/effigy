# g10.012 — Doctor subprocess deadlines

Status: ready
Owner: doctor, scan, and dependency maintainers
Created: 2026-09-24
Governing refs: architecture `029`, contract `047`, completed task `g10.010`
Depends on: none

## Outcome

The structural and deep doctor deadlines cover subprocess work as well as the
cooperative checks and health task. A stalled Cargo metadata or Git identity
child cannot hold doctor beyond its selected overall budget or survive it.

## Ready-State Rubric

- [x] The operator approved the release-audit repair frontier on 2026-09-24.
- [x] Architecture `029` and contract `047` already settle the single deadline,
  partial report, and process cleanup behavior.
- [x] Mutable ownership, adversarial subprocess proof, and release boundary
  are explicit.
- [x] This is independent of `g10.011` and `g10.013`.
- [x] No new product choice remains for implementation.

## Decisions

- Carry the same remaining monotonic deadline through dependency health and
  deep Git identity subprocesses. Check after a subprocess only as a secondary
  guard; the subprocess itself must be bounded and reaped.
- Preserve the structural/deep split, selected catalog scope, `0` unbounded
  override, exact cache rules, and additive report contract.
- Use production-path subprocess fixtures; `RunnerDoctorPorts` unit tests
  bypass the bounded child path under `#[cfg(test)]`.

## Dispatch manifest

- **State:** ready; independent parallel siblings `g10.011` and `g10.013`.
- **Completion:** one reviewed PR proves fast and deep subprocess deadline
  closure, existing doctor behavior, `[Unreleased]` update, merge, and
  hook-owned closeout.
- **Owned mutable paths:** `crates/effigy-doctor/src/checks.rs`,
  `dependency_health.rs`, and focused doctor tests;
  `crates/effigy-scan/src/execution/doctor_inventory.rs` and its focused tests;
  `crates/effigy-deps/src/status.rs`, `crates/effigy-deps/src/cargo.rs`, and
  `crates/effigy-deps/src/process.rs` only for a bounded doctor
  adapter; `src/runner/doctor_ports.rs`; relevant doctor CLI subprocess tests
  under `tests/cli_output_tests/` or `src/tests/runner_tests/`;
  `docs/guides/018-doctor-explain-mode.md` or `063-container-system-guide.md`
  only if the operator path needs clarification; `CHANGELOG.md`, `PAPERCUTS.md`,
  and this task's evidence.
- **Reserved closeout surfaces:** generation/front-door indexes, lifecycle
  records/projections, and the submitted handoff are hook/Chatterbox owned.
  `CHANGELOG.md` is shared with parallel siblings; merges must serialize and
  retain every independent entry.
- **Worker:** complex Rust process/cancellation work in the automatic Queue
  pool; independent exact-head review required.
- **Excluded:** scan-policy changes, cache pruning, new doctor modes/defaults,
  repository `health` rewrites, release mutation, workflows, and unrelated
  dependency semantics.
- **Escalation:** Chatterbox for any new timeout, report, process, or catalog
  policy decision.

## Work

1. Trace every blocking step from doctor entry to deadline report, including
   dependency health and Git identity, and add focused blocked-child fixtures.
2. Propagate the remaining overall budget into those subprocesses, terminate
   and reap the owned process tree on expiry, and preserve prior cache state.
3. Verify default and deep modes report a non-zero incomplete result with the
   named phase and completed evidence, in text and JSON.
4. Run existing doctor/scan/dependency regressions and proportionate QA;
   update `[Unreleased]` and operator guidance only where behavior changed.

## Acceptance and review oracle

| Invariant | Adversarial counterexample | Required proof |
| --- | --- | --- |
| Fast doctor is bounded | Linked Cargo dependency inspection launches blocked metadata after registered checks | Real CLI fixture with an owned marker-writing descendant returns within a small override budget, non-zero with partial phase evidence, and leaves no child |
| Deep Git identity is bounded | `git ls-files` or `git status` stalls before inventory's first budget check | Instrumented deep fixture bounds each child by remaining budget, reports the phase, and leaves no child or partial cache publication |
| One deadline remains one deadline | Each subprocess receives a fresh 10/120-second allowance and total elapsed overruns | Chained delays prove total budget is shared; `0` override remains deliberately unbounded and visible |
| Existing diagnosis survives | Cancellation drops completed findings, changes scan order, touches sibling catalogs, or turns timeout into success | Text/JSON report, catalog-isolation, cache-integrity, doctor explain/`--fix`, scan parity, and health regressions stay green |

## Validation

- focused doctor, scan inventory, subprocess, dependency, and CLI tests;
- `cargo fmt --all -- --check` and `cargo clippy --all-targets -- -D warnings`;
- JSON contract checks, `effigy qa:docs`, and proportionate workspace QA;
- `git diff --check` and independent exact-head review.

## Stop conditions

- Stop if a child process tree cannot be terminated and reaped through owned
  execution primitives, or if a truthful partial report needs a contract change.
- Stop if selected-catalog isolation or prior valid cache state cannot be
  preserved.
- Stop before widening doctor scope, changing defaults, or mutating releases.

## Evidence

On completion record blocked-child timing, process cleanup, cache preservation,
validation, PR and reviewed exact head, merge commit, and remaining limits.

## Next task

After closeout, wait for the other approved release repairs and return to
Chatterbox for 0.13.0 readiness. No release mutation follows automatically.
