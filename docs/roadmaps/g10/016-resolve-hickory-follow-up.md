# g10.016 — Resolve Hickory 0.26.3 follow-up

Owner: Cargo dependency maintenance with gateway consumers
Created: 2026-09-24
Governing refs: `docs/contracts/001-working-rules.md`, `deny.toml`, `docs/guides/049-ci-binary-distribution-and-release-protocol.md`
Depends on: `g10.015` terminal closeout

## Outcome

One reviewed current-base PR advances `hickory-proto` and `hickory-server` from 0.26.2 to 0.26.3 after the direct dependency batch, with the resolver delta and gateway behavior checked. The updated Dependabot targets receive accurate dispositions.

## Ready-State Rubric

- [x] The operator requested resolution of the Dependabot backlog through consolidated reviewed batches.
- [x] Dependabot retargeted #97 and #100 to 0.26.3 after the first batch was prepared; #117 covers only 0.26.2.
- [x] `g10.015` owns `Cargo.lock` first; Queue must enforce serial execution.
- [x] Versions, validation, source PR disposition, and stop conditions are explicit.

## Decisions

- Treat the 0.26.3 retargets as new dependency work, separate from merged PR #117. Do not claim #117 supersedes them.
- #97 is closed and cannot reopen because GitHub reports its Dependabot branch deleted. Preserve its history and include its 0.26.3 target in the replacement PR. #100 stays open until the replacement merges.
- Keep manifests, unrelated package versions, workflows, and release surfaces unchanged unless the resolver requires a bounded transitive movement recorded in evidence.

## Dispatch manifest

- **State:** ready after `g10.015` terminal closeout; no concurrent `Cargo.lock` writer.
- **Completion:** one independently reviewed current-base PR merges both 0.26.3 updates, with green CI; #100 closes as superseded with the merge link and #97 receives a correction linking the replacement merge.
- **Owned mutable paths:** `Cargo.lock`; focused gateway/Hickory consumer tests only if a compatibility assertion is missing; this task's evidence under `docs/logs/2026-09/`; `PAPERCUTS.md` for execution friction.
- **Reserved closeout surfaces:** task status, lifecycle records/projections, generation and docs front doors, and the submitted handoff are Queue/hook/Chatterbox owned.
- **Worker:** general Rust/Cargo maintenance in the automatic Queue pool; independent exact-head review.
- **Excluded:** unrelated dependency updates, manifest policy changes, workflow edits, release mutation, and alterations to the already merged #117.
- **Escalation:** Chatterbox if 0.26.3 requires a wider API or behavior change.

## Work

1. Start from the merged `g10.015` lockfile. Inspect current #100 and the closed #97 histories and verify their 0.26.3 target and current main versions.
2. Apply precise `hickory-proto` and `hickory-server` 0.26.3 updates. Inspect every lockfile version and feature change, including `hickory-net` and resolver-required transitives.
3. Validate gateway DNS behavior, locked workspace tests, deny, format, clippy, docs, and current-base CI. Open one non-draft aggregate PR linking #97 and #100.
4. After the replacement merges, close #100 as superseded with its merge link. Add the replacement link to #97's correction thread; do not erase the mistaken close/reopen history.

## Acceptance and review oracle

| Invariant | Adversarial counterexample | Required proof |
| --- | --- | --- |
| Accurate target | PR #117's 0.26.2 result is mistaken for the retargeted 0.26.3 work | Locked tree shows both 0.26.3 packages; aggregate PR names the two updated bot targets |
| Bounded resolution | Cargo silently changes unrelated package versions or denied crates | Complete lockfile delta and `cargo deny check`; explain resolver-required transitives |
| Gateway compatibility | Hickory resolves but changes gateway DNS or startup behavior | Focused gateway tests and full workspace tests pass, with a targeted assertion if an observable gap appears |
| Honest disposition | #100 closes before replacement merge or #97 remains incorrectly described as superseded by #117 | Merge precedes #100 close; both source PR threads link the actual 0.26.3 replacement |

## Validation

- Focused gateway/Hickory tests and locked dependency tree checks.
- `cargo test --workspace --locked`, `cargo deny check`, `cargo fmt --all -- --check`, `cargo clippy --all-targets --locked -- -D warnings`, `effigy qa:docs`, `git diff --check`.
- Hosted CI and independent review on the exact aggregate PR head.

## Stop conditions

- Stop if 0.26.3 cannot resolve without unowned manifest/API changes, unrelated package updates, denied dependencies, or behavioral changes needing a product decision.
- Stop on concurrent `Cargo.lock` ownership or stale exact-head review.

## Evidence

Record bot PR retargeting and #97's failed reopen, before/after package versions and complete lockfile delta, validation, reviewed head, merge, and both source PR dispositions. Queue owns lifecycle closeout.

## Next task

Return to Chatterbox for backlog verification and the next strategic runway.
