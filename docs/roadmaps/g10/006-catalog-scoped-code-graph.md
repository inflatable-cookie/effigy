# g10.006 — Catalog-scoped code graph

Owner: code graph, manifest, and catalog routing
Created: 2026-09-15
Governing refs: `docs/architecture/027-catalog-scoped-code-graph.md`, `docs/contracts/045-catalog-scoped-code-graph-contract.md`, `docs/contracts/037-explicit-catalog-membership-contract.md`, `docs/contracts/041-documentation-graph-profile-contract.md`
Depends on: none

## Outcome

An Effigy monorepo can mark existing catalogs as segmented and optionally
independent, then run graph commands against one catalog without scanning or
opening any sibling catalog; unsegmented repositories remain compatible.

## Ready-State Rubric

- [x] Objective is one usable vertical slice with no storage-only intermediate.
- [x] Governing architecture and contracts fix grammar, selection, storage,
  documentation isolation, compatibility, and failures.
- [x] Mutable scope, acceptance, validation, evidence, and stop conditions are
  explicit.
- [x] The adversarial oracle proves the negative no-sibling-work guarantee.
- [x] Continuation returns to Chatterbox after merge and hook closeout.
- [x] Operator confirmed catalog-derived segmentation, opt-in independent
  storage, and implementation on 2026-09-15.

## Decisions

- Use `[catalog.graph]` with `segmented` and `independent` booleans.
- Catalog membership and aliases are the only topology and identity source.
- Root queries search the root/folded corpus after pruning segmented members.
- Cwd and `--catalog` select one scope; `--all-catalogs` is the only fan-out.
- Segmented scopes are always lazy. `independent` controls physical storage only.
- Documentation context keeps its repository-owned corpus.
- Reject segmented catalogs escaping the workspace root in v1.

## Dispatch manifest

- **State:** ready; sole frontier task; no dependency edge.
- **Completion:** focused manifest, routing, storage, refresh, query, CLI, JSON,
  docs-context, and compatibility proofs pass; independent exact-head review
  approves; PR merges; lifecycle hook publishes canonical closeout.
- **Owned mutable paths:** `crates/effigy-manifest/src/**`,
  `crates/effigy-routing/src/**`, `crates/effigy-codegraph/src/**`,
  `crates/effigy-cli/src/**`, `src/runner/graph_command.rs`,
  `src/runner/graph_time_budget.rs`, `src/cli/graph_watch_dispatch.rs`,
  `src/cli/output/**`, graph-focused files under `src/tests/**` and
  `tests/cli_output_tests/**`, `docs/guides/017-json-output-contracts.md`,
  `docs/guides/022-manifest-cookbook.md`,
  `docs/guides/025-command-reference-matrix.md`,
  `docs/guides/026-json-payload-examples.md`,
  `docs/guides/076-code-graph-and-agent-workflows.md`, root `README.md`,
  `.agents/skills/effigy/**`, `CHANGELOG.md`, `PAPERCUTS.md`, and this task's
  implementation evidence.
- **Reserved closeout surfaces:** `docs/architecture/000-overview.md`,
  `docs/architecture/027-catalog-scoped-code-graph.md`,
  `docs/contracts/001-working-rules.md`,
  `docs/contracts/045-catalog-scoped-code-graph-contract.md`,
  `docs/contracts/README.md`, `docs/roadmaps/README.md`,
  `docs/roadmaps/g10/README.md`, `docs/roadmaps/generation-index.md`,
  `docs/specs/README.md`, `docs/logs/README.md`, lifecycle records/projections,
  and the submitted handoff are coordinator/hook owned.
- **Worker:** complex-capable Rust worker from the automatic pool; independent
  reviewer required. No by-request frontier profile is required.
- **Excluded:** new catalog discovery, recursive membership, arbitrary database
  paths, remote indexes, daemons, model-generated partitioning, release/workflow
  changes, catalog-pack behavior, and unrelated scan semantics.
- **Escalation:** Chatterbox owns any need to support escaping/external segmented
  catalogs, change defaults, merge documentation and code corpora, or widen the
  command surface beyond the contract.

## Work

1. Add and validate the composed `[catalog.graph]` manifest posture, exposing it
   through effective catalog resolution without changing membership.
2. Introduce a catalog graph-scope descriptor and deterministic shared versus
   independent `GraphPaths`/lock selection.
3. Make walking, freshness, index records, deletion, and shared-store entities
   scope-aware; prune segmented descendants before descent.
4. Apply scope predicates to every graph query and relationship traversal while
   keeping docs-context corpus refresh independent.
5. Add common `--catalog` and `--all-catalogs` parsing, cwd selection, text/JSON
   evidence, watch/status/index behavior, and fail-fast diagnostics.
6. Update operator/agent docs, changelog, papercut disposition, and focused
   fixtures; run validation and open the Queue-managed PR.

## Acceptance and review oracle

| Invariant | Adversarial counterexample | Required proof |
| --- | --- | --- |
| One catalog never pays sibling cost | Sibling contains a blocking/huge tree and selected query touches it during freshness | Instrumented fixture proves no walk, stat, DB open, lock, index row, or result for sibling |
| Root pruning is structural | Root walk enters a segmented tree then filters its files | Walker fixture fails on descent and still indexes root/folded files |
| Shared storage stays scoped | Reindexing catalog A deletes B rows or B symbols outrank A | Two-catalog shared DB refresh/deletion and query isolation tests |
| Independent means physical isolation | Selecting Bovine opens the shared DB or a Farmyard DB | Path/lock/open probes show only the encoded Bovine database is touched |
| Selection is deterministic | Cwd, explicit alias, and root silently choose different corpora | Parser/runner tests cover precedence, unknown aliases, and conflict failures |
| Fan-out is never implicit | Plain `graph explore` refreshes all effective catalogs | Single-query spy plus explicit `--all-catalogs` per-scope report proof |
| Relationships do not cause refresh | A reference to sibling source indexes or opens sibling storage | Cross-catalog shared/independent fixtures retain unresolved boundary without sibling work |
| Docs authority is unchanged | Pruning a source catalog removes configured docs or refreshes its code | Docs-context fixture proves configured corpus and zero code-scope refresh |
| Compatibility holds | A single-catalog repo changes paths, schema, text, watch, or timeout semantics | Existing graph suite plus additive JSON assertions passes unchanged |
| Unsafe topology fails closed | `independent` without segmentation or an escaping root creates partial state | Preflight tests prove empty graph-state delta and actionable diagnostics |

## Validation

- focused manifest composition and schema tests for `[catalog.graph]`
- `cargo test -p effigy-codegraph`
- focused runner/CLI graph option, selection, JSON, timeout, and watch tests
- focused docs-context regression tests
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `effigy qa:docs`
- `effigy contracts check-json --fast`
- `git diff --check`

Do not require an Acowtancy full-root index as validation. Use an instrumented
large-sibling fixture to prove bounded selection, then record an optional
Acowtancy catalog smoke only if available without widening scope.

## Stop conditions

- Stop if one-catalog selection still requires root-wide freshness or schema
  migration cannot isolate sibling records safely.
- Stop if docs context cannot retain its contract without a new operator-owned
  corpus decision.
- Stop if implementation requires recursive discovery, external catalog
  support, arbitrary storage paths, release/workflow edits, or unrelated scan
  changes.
- Stop on concurrent edits to an owned path or a contradiction with contracts
  `037`, `041`, or `045`.

## Evidence

On completion, record outcome, exact validation, no-sibling instrumentation,
storage paths touched, JSON compatibility, PR link, reviewed exact head, merge
commit, and material limits or blockers.

## Next task

Return to Chatterbox after hook-owned closeout. Decide Acowtancy adoption and
external-catalog support from real consumer evidence; neither is pre-authorized.
