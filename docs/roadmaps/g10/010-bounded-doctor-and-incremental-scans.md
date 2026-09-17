# g10.010 — Bounded doctor and incremental scans

Status: ready
Owner: doctor and scan maintainers
Created: 2026-09-17
Governing refs: architecture `029`, contract `047`, contracts `037` and `045`
Depends on: none

## Outcome

`effigy doctor` becomes a seconds-bounded structural check. Explicit
`effigy doctor --deep` performs catalog-scoped health and content diagnosis
through one shared inventory, exact incremental cache facts, and an overall
deadline that fails with useful partial evidence instead of hanging silently.

## Ready-State Rubric

- [x] The default/deep boundary and timeout defaults are settled.
- [x] Catalog selection reuses effective catalog membership.
- [x] Cache identity, publication, invalidation, and refresh are explicit.
- [x] Process-tree cancellation and incomplete-result behavior are explicit.
- [x] Compatibility, acceptance, adversarial review, evidence, continuation,
  and stop conditions are defined.
- [x] No product or operator choice remains for implementation.

## Decisions

- Default doctor is structural only; content scans and `health` are deep-only.
- Fast and deep default budgets are 10 seconds and 120 seconds.
- Deep scan work uses one physical inventory per selected catalog scope.
- Exact per-file facts are cached under `.effigy/doctor/cache/v1/`.
- Root scope prunes declared members; whole-repository fan-out is explicit.

## Dispatch manifest

- **State:** ready; sole g10 frontier task.
- **Completion:** one reviewed PR implements architecture `029` and contract
  `047`, passes focused adversarial proofs and proportionate workspace QA, and
  records the behavior change under `[Unreleased]`.
- **Owned mutable paths:** doctor CLI/routing and reports; doctor and scan
  crates/modules; catalog selection adapters; execution cancellation adapters;
  focused fixtures/tests; doctor/scan command docs; `CHANGELOG.md`; this task's
  evidence and directly affected active indexes.
- **Reserved closeout surfaces:** lifecycle projection blocks and terminal
  handoff consumption remain hook-owned.
- **Worker:** complex Rust implementation with independent exact-head review.
- **Excluded:** scan policy changes, automatic catalog discovery, remote cache,
  cache pruning, daemon/watch mode, unrelated health-task rewrites, workflows,
  release mutation, and changes to already-complete g10 tasks.
- **Escalation:** operator through Chatterbox for any timeout-default,
  compatibility, scope, or cache-trust change.

## Work

1. Split doctor orchestration into structural and explicit deep tiers. Preserve
   explanation mode and structural `--fix`; add help/completion and early flag
   conflict validation.
2. Reuse effective catalog membership to select root, cwd member, explicit
   alias, or explicit all-catalog scopes without sibling leakage.
3. Build one ordered streaming inventory per selected scope and adapt enabled
   scan evaluators to consume shared observations without changing findings.
4. Add exact per-file facts, versioned cache identity, atomic locked
   publication, corruption recovery, and `--refresh`.
5. Propagate one monotonic deadline through structural checks, inventory,
   evaluation, and health. Terminate and reap an owned health process tree.
6. Extend text/JSON reports with mode, scope, timing, completeness, per-check
   state, cache counts, and timeout evidence.
7. Update doctor/scan guidance and `[Unreleased]`; add focused fixtures and
   validation evidence.

## Acceptance and review oracle

| Invariant | Adversarial counterexample | Required proof |
| --- | --- | --- |
| Default doctor is fast and structural | An enabled scanner walks an 18k-file fixture or a health sentinel runs | Instrumented Acowtancy-shaped fixture completes inside 10 seconds with zero content walks and untouched sentinel |
| Deep scans share one walk | Five enabled scanners each construct their own walker | Cold-run traversal counter is exactly one per selected scope and findings match standalone evaluators |
| Warm results are exact | Unchanged files are reread, or same-size/same-mtime edits stale-hit | Read/analyse counters plus changed-content fixture prove reuse and invalidation |
| Catalog isolation holds | Selecting Dairy scans Bovine or writes its cache | Filesystem/cache/health sentinels prove no sibling touch; all-catalog mode visits each declared scope once |
| Timeout is terminal evidence | Caller sees silence, success, partial cache, or an orphan health child | Raw text/JSON subprocess tests prove non-zero partial report, named phase, intact prior cache, and reaped process tree |
| Cache failure is safe | Corrupt state panics or suppresses findings | Corruption and version-mismatch fixtures warn, rebuild, and match cold findings |
| Existing narrow surfaces remain compatible | Explain mode or standalone scan output changes accidentally | Focused snapshots/JSON contracts and existing doctor, scan, task, and catalog regression suites |

An optional read-only run against the local Acowtancy checkout may supplement
the deterministic fixture. It is evidence, not a required external dependency.

## Proportionate validation

- focused doctor CLI, report, timeout, health-cancellation, scan, cache, and
  catalog-isolation tests;
- existing standalone scan parity and doctor explanation/`--fix` regressions;
- CLI help/completion and JSON contract validation;
- `cargo test` for affected crates, then workspace `cargo test` after the
  coherent implementation batch;
- `cargo fmt --all -- --check`;
- `cargo clippy --all-targets -- -D warnings`;
- `effigy qa:docs` and `git diff --check`.

## Stop conditions

- Stop if health cannot be terminated and reaped through Effigy's owned
  execution primitives.
- Stop if cache hits cannot be established from exact content or Git identity.
- Stop if selected-catalog work needs to inspect or mutate sibling scopes.
- Stop if default structural doctor cannot meet the 10-second bound without a
  new product-policy decision.
- Stop before changing scan semantics, timeout defaults, workflows, releases,
  or unrelated repository health definitions.

## Evidence

On completion record the exact PR head and merge commit, cold/warm traversal
and read counters, default and deep durations, timeout/process cleanup proof,
cache-corruption proof, catalog-isolation proof, validation commands, and any
remaining limits.

## Next task

After hook-owned closeout, return to Chatterbox. No successor is pre-approved.
