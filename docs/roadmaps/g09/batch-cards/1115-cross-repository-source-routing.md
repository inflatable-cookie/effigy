# 1115 - Cross-Repository Source Routing

Roadmap: [`../006-cross-repository-source-routing.md`](../006-cross-repository-source-routing.md)
Spec: [`../../../specs/122-cross-repository-source-routing-strict-lane.md`](../../../specs/122-cross-repository-source-routing-strict-lane.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md), [`../../../contracts/041-documentation-graph-profile-contract.md`](../../../contracts/041-documentation-graph-profile-contract.md), [`../../../contracts/037-explicit-catalog-membership-contract.md`](../../../contracts/037-explicit-catalog-membership-contract.md)

Status: Queued (ready when card `1114` merges)
Owner: `[docs_policy.sources]` grammar, portfolio enumeration, `docs context
--sources` surface, grouped payload and text, source identity, fixtures and
benchmark freeze, starter opt-in, manual replay
Created: 2026-09-05
Queued since: 2026-09-05 operator confirmation; serial after `1114`

## Purpose

Route one query across the repositories that opted in under named
directories, return exact sections grouped per repository with identity, and
prove it on fixtures before any portfolio claim.

## Work

1. **Grammar.** Add typed `[docs_policy.sources]` (`share`, `front_doors`,
   `skill_roots`) to `effigy-manifest` with validation per spec `122`; add
   the `[portfolio] directories` file parser.
2. **Enumeration.** Resolve named directories one level deep; classify each
   child as shared, not-shared, missing, or invalid; skip hidden, `.paseo`,
   `worktrees`, `node_modules`, `target`; never follow symlinks out.
3. **Surface.** `--sources <PATH>` and repeatable `--only <HANDLE>` on
   `docs context`; help text; usage errors for a bad portfolio file.
4. **Routing.** Query each shared repository sequentially through the
   existing `docs_context` entry point with its own store, lock, freshness,
   and timeout; map outcomes to the status vocabulary with next steps.
5. **Payload.** `effigy.docs.context.sources.v1` and grouped text output;
   per-result `content_identity` and per-repository current/indexed HEAD.
6. **Fixtures and benchmark.** Two fixture repositories and a portfolio file;
   frozen cases for every status and negative control; freeze history line.
7. **Opt-in.** Effigy's `effigy.toml` declares `share = true` and front
   doors; the Northstar init/starter profile emits the block.
8. **Replay.** With no concurrent graph process, time three shared
   repositories warm; replay K1–K5 (K5 pending if unsettled) against
   Northstar, Effigy, Underlay read-only, and record the `rg` comparison.
9. **Close.** Guides `079`, `017`, `026`; contract `041` command contract
   and drift trigger; `CHANGELOG.md` `[Unreleased]`; one evidence log.

## Acceptance

- [ ] a directory with a shared, a not-shared, and a non-repository child
      yields exactly one searched repository and reports the other two
- [ ] `--only` with an unknown handle reports `disallowed`; a missing
      directory reports `missing`; both leave healthy repositories answering
- [ ] a forced timeout on one fixture repository is reported and the other
      repository's results are returned; exit 0
- [ ] every result carries handle, path, span, current HEAD, indexed HEAD,
      and `content_identity`; a dirty fixture file is `working-tree`
- [ ] results are grouped per repository in directory order; no merged list
- [ ] single-repository payload, ranking, and all existing benchmark cases
      unchanged; new cases frozen
- [ ] three shared repositories warm inside 5 s total on the reference machine
- [ ] Effigy opts in; the starter profile emits the block; no consumer edited
- [ ] evidence log holds the fixture matrix and the K1–K5 / `rg` replay table
      with no speedup or recall claim beyond what it shows

## Review Oracle

Falsify these counterexamples before PR creation:

1. An un-opted-in or nested repository was searched, or a symlink escaped.
2. Two repositories' results were merged or their authority compared.
3. One failing repository hid a healthy one, or was omitted from the report.
4. A result lacks identity fields, or working-tree content was labelled
   committed.
5. Single-repository payload or benchmark ranks changed.
6. Three-repository warm timing exceeds 5 s.
7. A speedup or recall claim appears without the comparison table.
8. A cache, shared index, parallel executor, env var, glob, or consumer
   write appeared.

## Validation

- focused manifest, enumeration, runner, payload, and text tests
- `effigy perf:docs-context-benchmark` with the new freeze
- three-repository warm timing, no concurrent graph process
- `effigy graph affected` for changed source, then direct targets
- `effigy qa`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`

## Evidence Requirement

One dated closeout log under `docs/logs/2026-09/` with the fixture status
matrix, identity samples, the timing, the K1–K5 replay rows with `rg`
comparison (time to usable evidence, source correctness, bytes returned),
and validation output.

## Stop Conditions

Stop if the design needs recursion, a merged ranking, a shared index or
cache, parallel execution, a single-repository ranking or budget change, a
new environment variable, a consumer repository edit, or a contract `041`
change beyond the frozen flag, grammar, and payload.

## Next Task

Wait for `1114` to merge; then execute steps 1 to 9 and open the PR at the
exact reviewed head.
