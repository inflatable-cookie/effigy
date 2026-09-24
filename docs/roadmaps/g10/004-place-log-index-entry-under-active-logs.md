# g10.004 — Place log-index entries under Active logs

Owner: documentation policy
Created: 2026-09-14
Governing refs: `docs/architecture/010-package-map.md`, `docs/contracts/030-low-risk-deduplication-contract.md`, `docs/guides/029-docs-qa-checklist-and-validation.md`, `docs/logs/README.md`
Depends on: none

## Outcome

`effigy docs add-log-index` inserts a missing log entry as the first item of
the `## Active logs` section. It never appends the entry beneath `## Next Task`
or another later section, and repeat execution remains idempotent.

## Ready-State Rubric

- [x] Objective is bounded enough to finish without fresh planning decisions.
- [x] Governing refs establish the docs-policy owner, active-log front-door
  structure, and existing command behavior.
- [x] Scope, acceptance, validation, evidence, and stop conditions are explicit.
- [x] The review oracle covers exact section placement, absent/malformed
  headings, and idempotence.
- [x] Continuation returns to Chatterbox after independent closeout; no sibling
  dependency is implied.
- [x] Operator promoted this bounded papercut repair on 2026-09-14.

## Decisions

- Treat `## Active logs` as the insertion boundary and the following level-two
  heading as its hard end.
- Insert newest-first immediately after the heading and its existing blank-line
  separator.
- Fail without rewriting the file when the required section is absent or
  structurally ambiguous. Do not fall back to EOF or an archive-heading name.
- Preserve the existing path normalization, report schema, idempotence, and
  human/JSON command behavior.

## Dispatch manifest

- **State:** ready; parallel sibling `g10.005`; no task dependency edge.
- **Completion:** focused docs-policy and CLI proofs pass; independent exact-head
  review confirms placement and no unrelated documentation mutation; PR merges
  and the lifecycle hook performs canonical closeout.
- **Owned mutable paths:** `crates/effigy-docs-policy/src/lib.rs`,
  `crates/effigy-docs-policy/src/tests.rs`,
  `tests/cli_output_tests/command_behavior_tests.rs`,
  `docs/guides/029-docs-qa-checklist-and-validation.md`, `CHANGELOG.md`,
  `PAPERCUTS.md`, and this task's evidence.
- **Reserved closeout surfaces:** `docs/roadmaps/g10/README.md`,
  `docs/roadmaps/README.md`, `docs/roadmaps/generation-index.md`,
  `docs/contracts/001-working-rules.md`, `docs/logs/README.md`, lifecycle
  records/projections, and the submitted handoff are coordinator/hook owned.
- **Worker:** automatic adequate Rust pool; independent reviewer required.
- **Excluded:** log sorting beyond the inserted first item, archive compaction,
  changes to `docs check index`, command grammar or report schemas, generated
  lifecycle blocks, release/workflow changes, and portfolio skill sync.
- **Escalation:** Chatterbox owns any new section grammar, compatibility
  fallback, or rewrite beyond the exact insertion repair.

## Work

1. Replace the obsolete archive-marker/EOF insertion rule with an exact
   `## Active logs` section insertion rule in the docs-policy owner.
2. Add unit fixtures for populated and empty Active logs sections, a later
   `## Next Task`, missing/duplicate headings, and repeat execution.
3. Strengthen the CLI fixture so it proves the entry is inside Active logs and
   before Next Task, not merely before an archive marker.
4. Update the command guide, changelog, papercut disposition, and evidence.
5. Run the focused tests and proportionate repository validation, then open the
   Queue-managed PR for independent exact-head review.

## Acceptance and review oracle

| Invariant | Adversarial counterexample | Required proof |
| --- | --- | --- |
| Entry lands in Active logs | README contains `## Next Task` and no obsolete archive marker, so the helper appends after Next Task | Raw README fixture proves the entry span is after `## Active logs` and before the next H2 |
| Newest-first placement is stable | Existing entries stay ahead of the inserted entry | Populated-section unit test asserts the new bullet is first |
| Empty sections work | A blank Active logs section causes EOF fallback or malformed spacing | Empty-section golden assertion |
| Invalid structure fails closed | Missing or duplicate Active logs headings trigger a partial rewrite | Error result, byte-identical file, and non-zero CLI proof |
| Repeat execution is idempotent | A second run duplicates or moves the bullet | Two-run unit and CLI assertions |
| Existing command contract remains compatible | Path normalization, JSON schema, or human success text changes | Existing docs-policy and CLI regression tests remain green |

## Validation

- `cargo test -p effigy-docs-policy`
- `cargo test --test cli_output_tests cli_docs_add_log_index -- --nocapture`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `effigy qa:docs`
- `git diff --check`

## Stop conditions

- Stop if a supported repository intentionally has no unique `## Active logs`
  section and compatibility requires choosing another destination.
- Stop if the repair requires changing the public JSON schema, command grammar,
  or index-validation policy.
- Stop on concurrent edits to an owned path or a contract contradiction.

## Evidence

On completion, record outcome, validation run, PR link, reviewed exact head,
merge commit, and material limits or blockers.

## Next task

Return to Chatterbox after hook-owned closeout. `g10.005` is an independent
frontier sibling, not a continuation dependency.
