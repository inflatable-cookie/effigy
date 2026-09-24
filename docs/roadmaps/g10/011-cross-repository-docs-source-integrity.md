# g10.011 — Cross-repository docs-source integrity

Owner: documentation context and manifest maintainers
Created: 2026-09-24
Governing refs: architecture `024`, contract `041`, guide `079`
Depends on: none

## Outcome

`docs context --sources` uses committed consent, reports excerpt provenance
conservatively, and never gives two repositories one selectable handle. Missing
and unreadable portfolio directories have truthful, actionable statuses.

## Ready-State Rubric

- [x] The operator approved all four release-audit findings for the pre-0.13.0
  frontier on 2026-09-24.
- [x] Contract `041` settles committed consent, unique handles, status meaning,
  and conservative provenance; no product choice remains.
- [x] This task owns one source-routing integrity outcome and its exact tests.
- [x] Acceptance includes adversarial Git, I/O, and duplicate-handle cases.
- [x] Release preparation remains gated on reviewed merge and closeout.

## Decisions

- Read the neighbor's root `effigy.toml` from `HEAD` for sharing consent. A
  working-tree manifest may affect its later normal query only after committed
  consent grants membership. Git absence or read failure never grants it.
- Preserve basename handles. Reject duplicate handles across portfolio
  directories before querying any repository, including without `--only`.
- Preserve the existing grouped payload schema and status vocabulary. Report
  absent directories as `missing`; report other directory read failures as
  `invalid` with the actual reason. Do not discard child-enumeration errors.
- Treat Git path decoding or status uncertainty as working-tree identity, not
  a committed claim. Do not trust a clean path merely because a quoted or
  renamed porcelain record was parsed incorrectly.

## Dispatch manifest

- **State:** ready; independent parallel siblings `g10.012` and `g10.013`.
- **Completion:** one reviewed PR satisfies the source-routing oracle, updates
  the guide and `[Unreleased]`, merges, and receives hook-owned closeout.
- **Owned mutable paths:** `crates/effigy-manifest/src/config_sections/docs_policy.rs`;
  `crates/effigy-codegraph/src/git.rs` and its focused tests;
  `crates/effigy-codegraph/src/docs_context/sources.rs`, `sources_payload.rs`,
  and `sources_tests.rs`; `tests/cli_output_tests/docs_context_sources_tests.rs`;
  `src/runner/docs_command/context.rs` only if needed for accurate output;
  `docs/guides/079-documentation-graph-profiles-and-context.md`,
  `CHANGELOG.md`, `PAPERCUTS.md`, and this task's evidence.
- **Reserved closeout surfaces:** generation/front-door indexes, lifecycle
  records/projections, and the submitted handoff are hook/Chatterbox owned.
  `CHANGELOG.md` is shared with parallel siblings; merges must serialize and
  retain every independent entry.
- **Worker:** complex Rust/source-trust work in the automatic Queue pool;
  independent exact-head review required.
- **Excluded:** single-repository ranking or graph profile changes, portfolio
  crawling, new handle syntax or status/schema variants, network acquisition,
  release mutation, workflows, and unrelated documentation cleanup.
- **Escalation:** Chatterbox for a new sharing, handle, schema, or status policy.

## Work

1. Reproduce the four audit findings with real Git and CLI fixtures before
   changing their owner paths.
2. Make committed root-manifest bytes the only sharing decision. Prove local
   overlays/includes and dirty opt-in or opt-out cannot change it.
3. Parse dirty paths unambiguously, including quoted names and both rename
   endpoints, and keep uncertain identity conservative.
4. Reject duplicate handles before query callbacks. Distinguish missing from
   unreadable directories and surface child enumeration failures.
5. Update guide/help if behavior is clarified, `[Unreleased]`, and focused
   validation evidence; preserve normal grouped text/JSON contracts.

## Acceptance and review oracle

| Invariant | Adversarial counterexample | Required proof |
| --- | --- | --- |
| Consent comes from `HEAD` | Working-tree `share = true` opts in an uncommitted neighbor, or dirty `false` opts out a committed one | Real Git fixtures for both directions, absent committed manifest, include/overlay, and zero query/write effects without consent |
| Provenance is conservative | A dirty Unicode/quoted name or rename endpoint misses the parsed status set and reports `committed` | Raw text/JSON source fixture over tracked, staged, renamed, untracked, and quoted paths; Git failure returns working-tree identity |
| Handle selection is unique | Two named directories each contain `same-name`; `--only same-name` queries both | Collision fixture fails before any query callback, names the handle and directories, and changes no neighbor state |
| Statuses tell the truth | Permission or iterator I/O failure says the directory is absent, or hides a partial list | Missing and non-NotFound failures produce distinct `missing`/`invalid` evidence and next steps; healthy siblings remain queryable where no portfolio collision exists |
| Existing consumers remain stable | Single-repository retrieval, grouped schema, source isolation, or ordinary success output drifts | Existing docs-context sources, manifest, CLI, JSON contract, and docs QA suites pass |

## Validation

- focused `effigy-manifest`, `effigy-codegraph`, and docs-source CLI tests;
- `cargo fmt --all -- --check` and `cargo clippy --all-targets -- -D warnings`;
- `effigy qa:docs`, JSON contract checks, and proportionate workspace QA;
- `git diff --check` and independent exact-head review.

## Stop conditions

- Stop if committed opt-in cannot be proved without composing or mutating an
  unshared neighbor.
- Stop if a new handle grammar or JSON schema is required, or Git uncertainty
  cannot be represented conservatively within the existing contract.
- Stop on unrelated scan/ranking changes or a shared-path conflict that loses
  another task's changelog entry.

## Evidence

On completion record fixture results, validation, PR and reviewed exact head,
merge commit, and any remaining limits. Lifecycle closeout is hook-owned.

## Next task

After closeout, wait for the other approved release repairs and return to
Chatterbox for 0.13.0 readiness. No release mutation follows automatically.
