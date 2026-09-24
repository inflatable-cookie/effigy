# g10.013 — Skill nested stdio passthrough

Owner: skill task execution maintainers
Created: 2026-09-24
Governing refs: architecture `025`, contract `042`, completed task `g10.001`
Depends on: none

## Outcome

Every accepted `effigy skill run --stdio passthrough` host task shape,
including nested task and built-in steps, preserves the task's stdin, stdout,
stderr, and exit status without Effigy-added bytes.

## Ready-State Rubric

- [x] The operator approved the release-audit repair frontier on 2026-09-24.
- [x] Architecture `025` and contract `042` already settle raw transport and
  accepted/rejected task shapes.
- [x] Ownership, adversarial byte/status proof, and stop conditions are explicit.
- [x] This is independent of `g10.011` and `g10.012`.
- [x] No new product choice remains for implementation.

## Decisions

- Passthrough mode applies through nested dispatch and rendering, not only
  direct shell launch. Captured nested output cannot gain a newline or lose
  non-text bytes merely because Effigy forwarded it.
- The selected task's exact exit status, including a nested failing leaf,
  becomes Effigy's exit status. Keep ordinary text and JSON modes unchanged.
- Do not silently reject a previously accepted host task shape to avoid raw
  transport proof; escalate if the current nested API cannot carry it.

## Dispatch manifest

- **State:** ready; independent parallel siblings `g10.011` and `g10.012`.
- **Completion:** one reviewed PR proves raw nested transport and exact status,
  preserves existing skill isolation, updates `[Unreleased]`, merges, and
  receives hook-owned closeout.
- **Owned mutable paths:** `src/runner/skill_command.rs`,
  `src/runner/execute/sequence_run.rs` and directly called output helpers needed
  for exact byte/status propagation, plus focused
  runner tests; `crates/effigy-execution/src/lib.rs` only if typed output/status
  propagation requires it; `tests/cli_output_tests/skill_command_tests.rs` and
  skill task fixtures; `docs/guides/047-agent-and-cross-repo-adoption.md` only
  if operator guidance needs clarification; `CHANGELOG.md`, `PAPERCUTS.md`,
  and this task's evidence.
- **Reserved closeout surfaces:** generation/front-door indexes, lifecycle
  records/projections, and the submitted handoff are hook/Chatterbox owned.
  `CHANGELOG.md` is shared with parallel siblings; merges must serialize and
  retain every independent entry.
- **Worker:** complex Rust execution/transport work in the automatic Queue
  pool; independent exact-head review required.
- **Excluded:** new CLI grammar, JSON schema changes, wider runtime inheritance,
  consumer secret access, container/managed/TUI/concurrent skill shapes,
  release mutation, and workflows.
- **Escalation:** Chatterbox for a new accepted-shape, status, or compatibility
  decision.

## Work

1. Add process-level nested skill fixtures that expose the current newline
   and non-zero-status paths, with exact byte assertions.
2. Carry passthrough mode through nested task/built-in dispatch and preserve
   output bytes and exit status end to end; retain fail-closed preflight.
3. Prove direct shell and nested task/Rhai behavior, text/JSON isolation,
   named and explicit source resolution, and existing rejected shapes.
4. Update `[Unreleased]` and focused operator guidance; run proportionate QA.

## Acceptance and review oracle

| Invariant | Adversarial counterexample | Required proof |
| --- | --- | --- |
| Nested stdout/stderr are raw | A nested task emits bytes without `\n`; `render_nested_output` appends one or trims whitespace | Raw subprocess comparison of exact stdout and stderr byte vectors, including no final newline and empty output |
| Exit status is exact | A nested leaf exits 23 but the top-level skill command returns 1 or 0 | Process fixture asserts exit 23 with task-owned bytes; normal success returns 0 and preflight failure leaves stdout empty |
| Input is inherited | A nested step receives transformed or drained stdin | Binary or newline-free stdin fixture matches bytes at the leaf where supported |
| Modes and isolation stay separate | Passthrough adds a header/envelope or changes normal JSON/text, source root, target root, secrets, or rejected shapes | Existing skill routing/stdio fixtures and JSON contracts pass, plus focused named/explicit source and isolation regressions |

## Validation

- focused skill CLI subprocess and runner/execution tests;
- `cargo fmt --all -- --check` and `cargo clippy --all-targets -- -D warnings`;
- JSON contract checks, `effigy qa:docs`, and proportionate workspace QA;
- `git diff --check` and independent exact-head review.

## Stop conditions

- Stop if an accepted nested shape cannot preserve raw bytes or exact status
  without a new public policy decision.
- Stop if the fix changes ordinary text/JSON execution or weakens source,
  target, or secret isolation.
- Stop before widening task shapes, changing release state, or editing workflows.

## Evidence

On completion record exact byte/status fixture results, validation, PR and
reviewed exact head, merge commit, and material limits.

## Next task

After closeout, wait for the other approved release repairs and return to
Chatterbox for 0.13.0 readiness. No release mutation follows automatically.
