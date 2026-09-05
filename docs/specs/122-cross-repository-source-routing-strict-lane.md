# 122 Cross-Repository Source Routing Strict Lane

Status: Queued (serial after spec `121`)
Owner: Effigy orchestrator
Created: 2026-09-05
Roadmap: [`g09.006`](../roadmaps/g09/006-cross-repository-source-routing.md)
Ready card: [`1115`](../roadmaps/g09/batch-cards/1115-cross-repository-source-routing.md)
Contracts: [`041`](../contracts/041-documentation-graph-profile-contract.md),
[`037`](../contracts/037-explicit-catalog-membership-contract.md)
Architecture: [`024`](../architecture/024-repository-defined-documentation-graph.md)
Guides: [`079`](../guides/079-documentation-graph-profiles-and-context.md),
[`017`](../guides/017-json-output-contracts.md)

## Outcome

One `docs context` call can route a query across the opted-in repositories
under named directories, returning exact sections grouped per repository
with provenance, freshness, and commit identity, degrading per repository
instead of failing whole, with no new index, daemon, or crawling.

## Fixed Decisions

The roadmap's Frozen Decisions section is normative for grammar, surface,
execution, output, status vocabulary, identity, replay, and latency. In
addition:

- `[docs_policy.sources]` is typed manifest grammar in `effigy-manifest`:
  `share` (bool, default false), `front_doors` (list of repository-relative
  files, optional), `skill_roots` (list of repository-relative directories,
  optional). Unknown keys fail parsing. Paths must stay inside the repository
  after normalisation. A repository without the table is `not-shared`.
- The portfolio file is `[portfolio] directories = [...]`, paths relative to
  the file, each must exist. Unknown keys fail parsing. No globs, no
  absolute-path requirement, no recursion.
- Enumeration never reads a nested `effigy.toml` beyond the immediate child,
  never follows symlinks out of the named directory, and skips hidden
  directories and any directory named `.paseo`, `worktrees`, `node_modules`,
  or `target`.
- Each repository is queried through the existing `docs_context` entry point
  with its own graph store and lock; no shared store, no cross-repository
  cache, no second refresh model. The single-repository code path and its
  payload are unchanged.
- `effigy.docs.context.sources.v1` contains: query, portfolio path, named
  directories, applied budgets, and an ordered list of repository blocks
  (handle, path, status, next step when not ok, current HEAD, indexed HEAD,
  freshness, profile state, front doors, skill roots, results). Each result
  carries the single-repository result fields plus `content_identity`
  (`committed` or `working-tree`). Text output mirrors the grouping.
- Exit code 0 when at least one repository is `ok` or `empty`; usage error
  for a missing or unparsable portfolio file; failure when every repository
  is non-ok, with every status listed.
- Two fixture repositories under `tests/fixtures/docs-context-benchmark/`
  (one with a docs profile, one baseline) plus a portfolio file join the
  benchmark with frozen cases for every status and negative control. The
  freeze is recorded in the script history. Existing cases are untouched.
- Effigy's own `effigy.toml` opts in with `share = true` and its front
  doors; the Northstar init/starter profile emits an equivalent block so
  consumers opt in by committing it. No consumer repository is edited.
- Not allowed: recursion or globs in enumeration, a merged ranking, a global
  authority scale, parallel execution, a shared index, changes to
  single-repository ranking or budgets, a new environment variable, or any
  write outside Effigy and its fixtures.

## Dependency Runway

```text
card 1114 merged (identifier retrieval)
  -> 1115 manifest grammar, portfolio enumeration, grouped payload, identity,
          fixtures + benchmark freeze, starter opt-in, manual replay + rg comparison
  -> exact-head review and merge
  -> Chatterbox reviews the replay table with the operator; recall/speed claims only from it
```

One worker owns card `1115`. Use a frontier-capable implementation worker.
Material review remains with the orchestrator.

## Whole-Lane Review Oracle

Reject the lane if any counterexample survives:

1. A repository is searched without `share = true`, or enumeration descends
   below the immediate children of a named directory, or follows a symlink
   out of it.
2. Results from two repositories are merged into one ranked list, or an
   authority value from one repository is compared with another's.
3. A failed, missing, timed-out, or not-shared repository blocks results
   from a healthy one, or is omitted instead of reported with its status and
   next step.
4. A result lacks handle, path, span, current HEAD, indexed HEAD, or
   `content_identity`; or a working-tree excerpt is labelled `committed`.
5. The single-repository payload or ranking changes, or the benchmark's
   existing cases move.
6. Three shared repositories warm exceed 5 s total on the reference machine.
7. The evidence log claims speedup or recall without the `rg` comparison
   table, or carries K5 as written into a recall claim.
8. A second index, daemon, cache, parallel executor, environment variable,
   or consumer-repository write appears; or the portfolio file accepts globs
   or unknown keys silently.

Smallest counterexample set: one directory with a shared, a not-shared, and a
non-repository child; one `--only` miss; one missing directory; one dirty
fixture file; one same-term query across both fixtures; one timeout forced
with a tiny budget on one repository; one three-repository warm timing.

## Validation And Evidence

Card `1115` maps every oracle row to named proof. Run
`effigy perf:docs-context-benchmark` with the new freeze, focused manifest,
enumeration, payload, and runner tests, the three-repository warm timing with
no concurrent graph process, `effigy qa`, `cargo fmt --all -- --check`,
`cargo clippy --all-targets -- -D warnings`, and `git diff --check`. Write one
dated evidence log with the fixture matrix and the manual replay table
(K1–K5 with K5 pending if unsettled, plus `rg` comparison).

## Stop Conditions

Stop and return to the orchestrator if the design needs recursion, a merged
ranking, a shared index or cache, parallel execution, a change to
single-repository ranking or budgets, a new environment variable, a consumer
repository edit, or a contract `041` change beyond adding the flag, grammar,
and payload named here.

## Next Task

Queued behind spec `121`. Execute card `1115` once card `1114` has merged.
