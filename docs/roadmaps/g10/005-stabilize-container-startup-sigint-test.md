# g10.005 — Stabilize the container startup SIGINT test

Owner: container runtime test harness
Created: 2026-09-14
Governing refs: `docs/contracts/005-container-runtime-contract.md`, `docs/contracts/012-container-manager-contract.md`, `docs/contracts/015-runtime-operation-pipeline-contract.md`, `docs/architecture/022-runtime-architecture-sanity-audit.md`
Depends on: none

## Outcome

`cli_container_attached_session_handles_sigint_during_startup` deterministically
signals the intended startup window and proves the existing manager-owned clean
interrupt behavior without relying on equal wall-clock sleep and wait budgets.

## Ready-State Rubric

- [x] Objective is bounded to test-harness reliability; production interrupt
  and shutdown behavior is already fixed by current contracts.
- [x] Governing refs identify manager/runtime ownership and the required clean
  attached-interrupt result.
- [x] Scope, acceptance, validation, evidence, and stop conditions are explicit.
- [x] The review oracle covers early, intended-window, and late-signal races.
- [x] Continuation returns to Chatterbox after independent closeout; no sibling
  dependency is implied.
- [x] Operator promoted this bounded papercut repair on 2026-09-14.

## Decisions

- Keep production behavior unchanged. Repair the fixture, readiness signal, or
  wait strategy inside the CLI test harness.
- Signal only after a deterministic fixture-owned marker proves the slow
  startup child is active and before startup can complete.
- Use bounded waits with materially separate setup and observation budgets;
  never replace the race with an unbounded sleep or retry.
- Repeated focused execution is acceptance evidence, not a permanent ignored
  test or blanket timing tolerance.

## Dispatch manifest

- **State:** ready; parallel sibling `g10.004`; no task dependency edge.
- **Completion:** the focused test passes repeatedly and under its normal test
  binary route; independent exact-head review confirms test-only semantics; PR
  merges and the lifecycle hook performs canonical closeout.
- **Owned mutable paths:** `tests/cli_output_tests/command_behavior_tests.rs`,
  `tests/cli_output_tests/support.rs`, `CHANGELOG.md`, `PAPERCUTS.md`, and this
  task's evidence.
- **Reserved closeout surfaces:** `docs/roadmaps/g10/README.md`,
  `docs/roadmaps/README.md`, `docs/roadmaps/generation-index.md`,
  `docs/contracts/001-working-rules.md`, `docs/logs/README.md`, lifecycle
  records/projections, and the submitted handoff are coordinator/hook owned.
- **Worker:** automatic adequate Rust pool; independent reviewer required.
- **Excluded:** production signal or container lifecycle changes, ignored-test
  annotations, global test serialization changes, arbitrary timeout inflation,
  release/workflow changes, and portfolio skill sync.
- **Escalation:** Chatterbox owns any evidence that the production interrupt
  path—not the harness—is defective or that an owned-path expansion is needed.

## Work

1. Reproduce and trace the current fixture timing around the Colima invocation
   marker, delayed startup process, signal handler, and test wait deadline.
2. Add or refine a fixture-owned startup marker so the test observes the exact
   interrupt window with a bounded wait before sending SIGINT.
3. Preserve assertions for successful clean closeout and no transition to log
   following; add the smallest adversarial proof needed for marker ordering.
4. Run repeated focused executions plus the normal CLI test route, update the
   changelog, papercut disposition, and evidence, then open the Queue-managed PR.

## Acceptance and review oracle

| Invariant | Adversarial counterexample | Required proof |
| --- | --- | --- |
| Signal targets startup | Marker is written before the delayed startup process is actually active | Fixture assertion ties the marker to entry into the controlled delay |
| Waits stay bounded | Missing marker hangs the test indefinitely | A bounded helper failure names the missing startup phase |
| Scheduling variance does not decide the result | The marker wait and child delay both expire at three seconds | Repeated focused runs with separated budgets pass |
| Clean interrupt behavior remains asserted | Test passes merely because the child exits, without cleanup evidence | Existing success text and no-logs-follow assertions remain |
| Production semantics stay unchanged | Runtime signal code is edited to satisfy the fixture | Exact-head diff contains only declared test/docs evidence paths |
| Normal suite integration remains healthy | Repeated isolated execution passes but the test binary route fails | Focused test and ordinary `cli_output_tests` validation evidence |

## Validation

- repeat `cargo test --test cli_output_tests cli_container_attached_session_handles_sigint_during_startup -- --nocapture` at least ten times
- `cargo test --test cli_output_tests cli_container_attached_session -- --nocapture`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `effigy qa:docs`
- `git diff --check`

## Stop conditions

- Stop if reproduction or tracing identifies a production signal/cleanup bug.
- Stop if deterministic proof requires editing runtime/container source or
  weakening the asserted clean-interrupt outcome.
- Stop on concurrent edits to an owned path or a contract contradiction.

## Evidence

On completion, record outcome, validation run, repeated-run count, PR link,
reviewed exact head, merge commit, and material limits or blockers.

### Implementation (worker PR, pre-merge) — 2026-09-14

- **Outcome reached:** the fake Colima runtime starts the controlled delay as a
  child process, verifies it is active, and publishes its PID through a
  fixture-owned marker. The test waits for that marker with a bounded 10-second
  budget inside a separate 15-second startup delay before sending SIGINT.
- **Assertions preserved:** clean attached-session interrupt output remains
  required, and the test still proves startup stops before `logs --follow`.
  Production signal and container lifecycle code are unchanged.
- **Validation run:** ten repeated focused executions passed;
  `cargo test --test cli_output_tests cli_container_attached_session --
  --nocapture`, `cargo fmt --all -- --check`,
  `cargo clippy --all-targets -- -D warnings`, `effigy qa:docs`, and
  `git diff --check` passed.
- **PR:** [#108](https://github.com/inflatable-cookie/effigy/pull/108), with
  implementation commit `92259eb90`; independent review, merge commit, and
  canonical closeout remain pending.
- **Limits:** the optional graph-affected diagnostic was stopped after the
  stale local-install binary exceeded its expected response window; it is
  unrelated to the required validation and made no repository changes.

## Next task

Return to Chatterbox after hook-owned closeout. `g10.004` is an independent
frontier sibling, not a continuation dependency.
