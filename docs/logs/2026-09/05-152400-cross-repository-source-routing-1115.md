# Cross-Repository Source Routing 1115 Closeout

Status: complete
Created: 2026-09-05
Roadmap: [`g09.006`](../../roadmaps/g09/006-cross-repository-source-routing.md)
Spec: [`122`](../../specs/122-cross-repository-source-routing-strict-lane.md)
Batch: cross-repository-source-routing-1115
Contract: [`041`](../../contracts/041-documentation-graph-profile-contract.md)
Predecessor: [`05-133718`](./05-133718-docs-context-exact-identifier-1114.md)

## Summary

`effigy docs context <QUERY> --sources <PATH>` routes one query across the
repositories that opted in under the directories a portfolio file names, and
returns exact sections grouped per repository with per-repository provenance
and per-result commit identity. Membership is two-sided and both halves are
committed text. Enumeration is one level deep, sequential, and reuses the
unchanged single-repository entry point. No second index, daemon, cache,
parallel executor, environment variable, or consumer-repository write.

Three real-size shared repositories answer warm in 1.894 s, inside the 5 s
gate. The frozen benchmark grew from 13 to 23 cases; the 13 existing
single-repository cases are unedited and green. The K1–K4 replay is recorded
below with its `rg` comparison and its misses. K5 remains **pending**: the
triage that would rephrase it is still open, so it carries no recall claim.

## Measurement Conditions

| Field | Value |
| --- | --- |
| Machine | Mac17,7, 18 cores, macOS 26.6.2 |
| Checkout | `/Users/tom/.paseo/worktrees/310mya31/g09-006-cross-repository-source-routing-1115` |
| Binary | release profile, `v0.12.1+local.4a46987` |
| Implementation head | `4a469877e` |
| Northstar HEAD | `45791cdc0ee8f19d72bbf92d1f31bba44391e1d9`, clean |
| Effigy HEAD (main checkout) | `2615a7ac90c4e3719c3497911a23a341c9e37aec`, clean |
| Underlay HEAD | `97a26d9fa0a58daf198926ddcd259193daa9d5c3`, clean |
| Concurrent graph process | none during the timing block (`pgrep` reported 0) |
| Load average at timing | 3.80 / 6.30 / 9.55 before, 3.66 / 6.23 / 9.51 after |

The replay read the three sibling repositories only. Their tracked files,
manifests, and history were not modified. The single side effect is the
gitignored `.effigy/graph` index each query refreshes, which is the same
derived artifact the pilot produced and is regenerable from the checkout.

## Fixture Status Matrix

Committed fixture: `tests/fixtures/docs-context-benchmark/portfolio`. The
benchmark copies it to `.effigy/perf/docs-context-benchmark/portfolio-scratch`,
turns three children into real git checkouts, and mutates the copy to reach the
two states a clean committed tree cannot hold. Every row below is a frozen
benchmark case; all nine pass.

| Case | State | Observed blocks | Exit |
| --- | --- | --- | --- |
| `portfolio-membership` | clean | `baseline-notes=ok`, `loose-notes=invalid`, `private-vault=not-shared`, `shared-atlas=ok`, `absent-directory=missing` | 0 |
| `portfolio-negative-control` | clean | 3 results, none from `private-vault`, `loose-notes`, `worktrees/decoy-checkout`, or `.hidden-annex` | 0 |
| `portfolio-membership-boundary` | clean | `private-vault=not-shared`, no `.effigy` written into it | 0 |
| `portfolio-grouping` | clean | `baseline-notes/notes/README.md`, `shared-atlas/atlas/charters/tolerance-ledger.md`, `shared-atlas/atlas/notices/tolerance-ledger-withdrawn.md`; each block ranks from 1 | 0 |
| `portfolio-identity` | clean | every answered block carries both HEADs; every result carries a span and `committed` | 0 |
| `portfolio-only-disallowed` | clean | `shared-atlas=ok`, `no-such-repo=disallowed` | 0 |
| `portfolio-empty` | clean | both shared repositories `empty` | 0 |
| `portfolio-working-tree` | dirty charter | edited file `working-tree`, unchanged neighbour `committed` | 0 |
| `portfolio-stale` | duplicate single-valued field | `shared-atlas=stale` and still returning sections, `baseline-notes=ok` | 0 |
| `portfolio-timeout` | `EFFIGY_GRAPH_TIMEOUT_MS=1` | both shared repositories `timeout`, every status still listed | non-zero |

`worktrees/decoy-checkout` and `.hidden-annex` are opted-in repositories placed
inside a skipped container and a hidden directory. Retrieving either would mean
enumeration went where it must not; neither appears in any report.

### Membership is decided from committed bytes only

Review of PR `93` at `343f888b5` found that classification loaded each child's
*composed* manifest. That was a real defect against oracle rows 1 and 8, and it
is fixed at `5218fd340`:

- an uncommitted `effigy.local.toml` overlay saying `share = true` would have
  granted membership on text the repository never committed
- a declared `[bundle.base] type = "git"` would have been resolved during
  classification, cloning into `<neighbour>/.effigy/cache/bundles/git/...` — a
  write into a repository that never opted in

Both were reproduced before the fix. Composing the fixture's manifest directly
still shows the hazard the fix removes:

```text
$ effigy tasks --repo <copy-of-private-vault>
[error] strict manifest parse failed ...: git clone --no-checkout
  https://example.invalid/never-clone-me.git
  <copy-of-private-vault>/.effigy/cache/bundles/git/f3538197.../main failed
```

Classification now reads the committed bytes of the child's own `effigy.toml`
and nothing else. `private-vault` carries a git bundle, an include, and a local
overlay that all claim `share = true`; frozen case
`portfolio-membership-boundary` asserts it reports `not-shared`, returns no
section, and has no `.effigy` after the run. Effigy's own opt-in moved from the
included `docs/effigy.docs.toml` to the root `effigy.toml` for the same reason,
and is still honoured. Composition remains in place for a repository that has
already opted in, when it is queried.

### Degradation that does not hide a healthy repository

`portfolio-timeout` proves the failure exit rule (no repository answered). The
mixed case — one repository times out while another answers, exit 0 — is proved
by `one_repository_timing_out_never_hides_another_repository_answering` in
`crates/effigy-codegraph/src/docs_context/sources_tests.rs`, which injects the
timeout for exactly one repository. A fixture cannot force a per-repository
timeout deterministically, because the budget is per repository and a value
small enough to expire on one checkout expires on all of them.

## Identity Samples

From the clean benchmark state and the three-repository timing portfolio:

| Repository | current HEAD | indexed HEAD | Result | `content_identity` |
| --- | --- | --- | --- | --- |
| `shared-atlas` | committed fixture HEAD | equal | `atlas/charters/tolerance-ledger.md` | `committed` |
| `shared-atlas` (after edit) | unchanged | unchanged | `atlas/charters/tolerance-ledger.md` | `working-tree` |
| `shared-atlas` (after edit) | unchanged | unchanged | `atlas/notices/tolerance-ledger-withdrawn.md` | `committed` |
| `alpha` | `b94f1c80bdea` | `b94f1c80bdea` | 6 sections | `committed` |
| `beta` | `56a3b8c9bf94` | `56a3b8c9bf94` | 6 sections | `committed` |
| `gamma` | `56a3b8c9bf94` | `56a3b8c9bf94` | 6 sections | `committed` |

Identity is never optimistic. When git cannot answer, or the index carries no
clean stamp, the excerpt is reported as `working-tree` rather than claimed as
committed bytes.

## Three-Repository Warm Timing

Portfolio of three independent full clones of this branch (2436 scoped
documents and a 240 MB graph each — the largest opted-in corpus available, not
a reduced fixture). Each was indexed once beforehand; the runs below are warm,
sequential, and taken with no other graph process running.

Command: `effigy --json docs context "documentation graph profile contract" --sources <portfolio>`

| Run | Elapsed | Blocks | Exit |
| --- | --- | --- | --- |
| 1 | 1.969 s | `alpha=ok` (6), `beta=ok` (6), `gamma=ok` (6) | 0 |
| 2 | 1.906 s | same | 0 |
| 3 | 1.894 s | same | 0 |

Gate: three shared repositories warm inside 5 s total. **Met** — 1.894 s to
1.969 s, roughly 630 ms per repository, consistent with the `g09.005` warm
figure and with sequential execution.

## Manual K Replay

Read-only, one query per repository through the existing single-repository
route, `--max-sections 3 --max-bytes 6000`, `EFFIGY_GRAPH_TIMEOUT_MS=5000`.
Queries were written and frozen in `/tmp/k-replay/replay.sh` before the first
run. Expected sources are the pilot's, not chosen after seeing a result.

### Cold round

Repeats the pilot's finding rather than replacing it:

| Case | Repository | Elapsed | Result |
| --- | --- | --- | --- |
| K1 | Northstar | 5.107 s | typed graph timeout at 5000 ms, no evidence |
| K2 | Northstar | 5.103 s | typed graph timeout at 5000 ms, no evidence |
| K3 | Northstar | 5.110 s | typed graph timeout at 5000 ms, no evidence |
| K4 | Effigy | 5.113 s | typed graph timeout at 5000 ms, no evidence |
| UL probe | Underlay | 4.376 s | answered inside the budget |

### Warm round, after one `effigy graph index` per repository

| Case | Question | Elapsed | Top-3 paths | Bytes | Expected source in top 3 |
| --- | --- | --- | --- | --- | --- |
| K1 | Should I reuse this launcher-provided worktree? | 0.171 s | `docs/logs/archive/2026-08/31-152748-centralize-paseo-worktree-runtime.md`, `docs/roadmaps/archive/g02.md`, `docs/contracts/002-agent-local-paths.md` | 4892 | **no** |
| K2 | Where is the AGENTS review workflow? | 0.176 s | `skills/northstar/references/modes/agent-instruction-review.md`, `skills/northstar/references/router.md`, `docs/roadmaps/archive/g02.md` | 5874 | **yes**, rank 1 |
| K3 | Where does a consumer get the batch-card template? | 0.177 s | `bundle-docs/batch-card-relocation-guide.md`, `template-bundle/roadmaps/README.md`, `bundle-docs/sections/08-specs-and-promotion.md` | 5830 | **no** (related, not the named skill-shipped asset) |
| K4 | Which JSON field lists Effigy tasks? | 0.694 s | `docs/logs/2026-09/05-133718-docs-context-exact-identifier-1114.md`, `docs/guides/026-json-payload-examples.md`, `docs/triage/20260905-docs-context-identifier-retrieval-and-k5-expectation.md` | 3382 | **yes**, rank 2 |
| K5 | — | — | — | — | **pending** |

Two of four expected sources land in the top three. K1 and K3 miss, and the
misses are recorded, not diagnosed away: K2's rank-1 hit came from `skills/`,
so the miss is a ranking outcome rather than a scoping one. Nothing here
supports a recall claim; the table is the whole claim.

K5 is pending. The open triage
[`20260905-docs-context-identifier-retrieval-and-k5-expectation`](../../triage/20260905-docs-context-identifier-retrieval-and-k5-expectation.md)
records that the question as written is not a valid exact-section retrieval
oracle. The Underlay row above is a route-health probe on a different query and
carries no K5 result.

### `rg` comparison

| Case | `docs context` warm | `docs context` bytes | `rg` | `rg` bytes | `rg` lines |
| --- | --- | --- | --- | --- | --- |
| K1 | 0.171 s | 4892 | 0.037 s | 951 | 4 |
| K2 | 0.176 s | 5874 | 0.032 s | 1996 | 9 |
| K3 | 0.177 s | 5830 | 0.030 s | 3798 | 20 |
| K4 | 0.694 s | 3382 | 0.082 s | 13182 | 83 |
| UL probe | 0.376 s | 2830 | 0.065 s | 4118 | 26 |

`rg` is faster on every row here, and the comparison is deliberately generous
to it: each `rg` pattern is the reference source's own hyphenated token
(`launcher-provided worktree`, `agent-instruction-review`,
`batch-card-template`), chosen with the answer already known. `docs context`
was given the natural-language question instead. `rg` returned matching lines;
`docs context` returned whole sections with path, span, kind, authority,
currentness, and — under `--sources` — repository identity. **No speedup claim
is made, in either direction.** The two tools return different things, and the
only measured advantage in this table belongs to `rg`.

## Two-Sided Membership On The Real Portfolio

`effigy --json docs context "batch card template" --sources ~/Dev/projects --only northstar --only effigy --only underlay`

| Handle | Status | Next step |
| --- | --- | --- |
| `effigy` | `not-shared` | declare `[docs_policy.sources] share = true` |
| `northstar` | `not-shared` | declare `[docs_policy.sources] share = true` |
| `underlay` | `not-shared` | declare `[docs_policy.sources] share = true` |

Exit non-zero, every status listed, nothing searched, nothing written. This is
the membership rule working: a portfolio naming a directory grants no access.
Effigy's own opt-in lives on this branch and is not yet on the main checkout,
which is why it reports `not-shared` here too.

## Benchmark

`effigy perf:docs-context-benchmark` (ninth freeze, 23 cases): all hold.

| Case | Rank | Rival rank | Result |
| --- | --- | --- | --- |
| `generic-charter-authority` | 1 | 2 | pass |
| `generic-current-over-retired` | 1 | 2 | pass |
| `generic-historical-direct` | 1 | — | pass |
| `generic-authority-gate` | — | — | pass |
| `generic-no-match` | — | — | pass |
| `generic-relation-follow-up` | 4 | — | pass |
| `generic-exact-identifier` | 1 | 2 | pass |
| `effigy-contract-authority` | 1 | — | pass |
| `effigy-architecture-authority` | 1 | — | pass |
| `effigy-direct-historical-guide` | 1 | — | pass |
| `effigy-next-task` | 1 | — | pass |
| `effigy-historical-decision` | 2 | — | pass |
| `effigy-exact-identifier` | 2 | — | pass |

Every case, query, expected source, rival, and rank bound is byte-identical to
the seventh freeze. One observed rank moved: `effigy-exact-identifier` was 1 at
card `1114` and is 2 here. Diagnosis, verified by running the query directly:
rank 1 is now `docs/logs/2026-09/05-133718-docs-context-exact-identifier-1114.md`
at relevance 81, card `1114`'s own evidence log, which merged after that
measurement and documents the `catalog_tasks` result. Guide `026` follows at
relevance 69. This is a live-target corpus change, which the freeze history
already records as expected for `effigy-live` rows; the frozen bound is rank
≤ 3 and it holds. No ranking rule, authority weight, or budget default changed.

## Oracle Mapping

| Spec `122` counterexample | Proof |
| --- | --- |
| 1. searched without `share = true`, or descended below immediate children, or followed a symlink out | `only_opted_in_children_are_searched_and_the_others_are_reported`, `enumeration_stays_one_level_deep_and_skips_container_directories`, `a_symlinked_child_leaving_the_named_directory_is_out_of_scope`, `an_opt_in_that_lives_only_in_an_include_does_not_grant_membership`; benchmark `portfolio-membership`, `portfolio-negative-control`, `portfolio-membership-boundary` |
| 2. results merged, or authority compared across repositories | `results_stay_grouped_per_repository_with_declared_membership_metadata`; benchmark `portfolio-grouping`; end-to-end `sources_routing_answers_opted_in_repositories_and_reports_the_rest` |
| 3. a degraded repository blocked or hid a healthy one | `one_repository_timing_out_never_hides_another_repository_answering`, `a_missing_directory_is_reported_without_silencing_a_healthy_one`, `a_degraded_index_reports_stale_and_a_no_match_reports_empty`; benchmark `portfolio-stale`, `portfolio-membership` |
| 4. a result lacks identity, or working-tree content labelled committed | `content_identity_never_claims_committed_bytes_without_git_evidence`, `a_working_tree_excerpt_is_never_labelled_as_committed_bytes`; benchmark `portfolio-identity`, `portfolio-working-tree` |
| 5. single-repository payload or ranking changed, or existing benchmark cases moved | `effigy.docs.context.v1` untouched; existing 13 cases unedited and green (table above) |
| 6. three shared repositories warm exceed 5 s | 1.894–1.969 s, measured above |
| 7. speedup or recall claim without the `rg` table, or K5 carried into a recall claim | replay section above; K5 recorded pending, `rg` comparison published with its bias stated |
| 8. second index, daemon, cache, parallel executor, environment variable, or consumer write; portfolio accepting globs or unknown keys | no new module opens a second store or spawns a parallel executor; the per-repository bound reuses `run_bounded_graph_value` in the one timeout model; `unknown_keys_globs_and_escapes_are_rejected`, `a_missing_or_unparsable_portfolio_is_a_usage_error`; classification writes nothing into a neighbour — `a_not_shared_neighbour_with_a_bundle_and_an_overlay_is_never_cloned_written_or_searched`, `membership_comes_from_the_root_manifest_not_an_overlay_or_an_include`, benchmark `portfolio-membership-boundary` |

## Validation

- `cargo test --workspace` — green
- `effigy perf:docs-context-benchmark` — 23/23
- `cargo fmt --all -- --check` — clean
- `cargo clippy --all-targets -- -D warnings` — clean
- `git diff --check` — clean
- `effigy docs check links` on the changed guides — passed
- `effigy docs check json-examples` — passed

## Next Task

Coordinator closeout: `CHANGELOG.md`, contract `041` command contract and drift
trigger, card `1115`, roadmap `g09.006`, spec `122`, `docs/logs/README.md`, and
merge. Chatterbox still owns the K5 rephrasing.
