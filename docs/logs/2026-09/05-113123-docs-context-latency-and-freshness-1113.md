# Docs Context Latency and Freshness Evidence

Status: complete
Created: 2026-09-05
Roadmap: [`g09.005`](../../roadmaps/g09/005-docs-context-latency-and-freshness.md)
Spec: [`120`](../../specs/120-docs-context-latency-and-freshness-strict-lane.md)
Batch: docs-context-latency-and-freshness-1113
Contract: [`041`](../../contracts/041-documentation-graph-profile-contract.md)

## Summary

`effigy docs context` missed the frozen warm budget on this repository because
of glob recompilation, not because of the index, the lock, or the refresh
model. Compiling each distinct `docs_policy.graph` pattern once per process
cut a warm query from a p50 of 1935 ms to 602 ms on a current index and from
2045 ms to 682 ms with a dirty working tree, cut a 50-file stale refresh from
12.8 s to 10.2 s, and made cold `graph index` 19% faster. The timeout detail
now names the phase the bound expired in.

No second index, daemon, embedding, flag, environment variable, default-budget
change, contract `041` semantic change, or schema id bump. The frozen
`perf:docs-context-benchmark` matrix is unedited and green, and every K4/K5/
no-match replay row is byte-identical before and after, which is the proof the
repair is latency-only.

## Measurement Conditions

Identity shared by every row unless stated otherwise.

| Field | Value |
| --- | --- |
| Machine | Mac17,7, 18 cores, macOS 26.6.2 |
| Checkout | `/Users/tom/.paseo/worktrees/310mya31/g09-005-docs-context-latency-1113` (worker worktree of `effigy`) |
| Before binary | `v0.12.1+local.7f75092`, release profile, built from the lane base |
| Before source SHA | `7f75092d669c3281a674fdf2a3f6097b67d3b523` |
| After binary | `v0.12.1+local.696200a`, release profile |
| After source SHA | `696200af3` (this lane's repair) |
| Query | `docs context "catalog_tasks" --max-sections 3 --max-bytes 6000` |
| Concurrent graph process | none; `pgrep` showed no `effigy graph` process and no holder of `.effigy/graph/refresh.lock` for the duration |
| Timing | wall/user/sys measured per process via `resource.getrusage(RUSAGE_CHILDREN)` deltas |

The machine was shared with unrelated agent sessions, so 1-minute load average
is recorded per block. Load moves absolute numbers; it does not move the
before/after ratio, and the two warm blocks below were taken four minutes
apart at comparable load.

### Effigy corpus

| Field | Before | After |
| --- | --- | --- |
| Indexed files | 3939 | 3940 |
| Tracked Markdown files | 2473 | 2473 |
| Scoped documents (profile `592812a4`) | 2425 | 2425 |
| Symbols / edges / references | 42180 / 208432 / 88558 | 42196 / 208586 / 88635 |
| Graph DB | 240 MB | 245 MB |
| Working tree | clean at base | clean at repair commit |

The single-file corpus growth is this lane's own new source file.

### Fixture corpus

`tests/fixtures/docs-context-benchmark/generic-handbook`: 8 indexed files,
7 Markdown, 43 symbols, 44 edges, 3 references.

## Before Table

| Condition | Budget | Wall | User | Sys | Load | Outcome |
| --- | --- | --- | --- | --- | --- | --- |
| Effigy cold `graph index`, empty `.effigy/graph` | unbounded | 182388 ms | 67849 ms | 100665 ms | 7.8 | ok, lane baseline |
| Effigy warm, current index, clean tree, 5 runs | 5000 ms | p50 1935 ms (1721/1838/1935/2171/2328) | ~1621 ms | ~348 ms | 17.6 | ok, target ≤ 2000 ms met but with no margin |
| Effigy warm, current index, dirty tree, 5 runs | 5000 ms | p50 2045 ms (2030/2038/2045/2113/2114) | ~1816 ms | ~289 ms | 18 | ok, **misses the ≤ 2000 ms target** |
| Effigy stale-incremental, 50 tracked Markdown files edited | 120000 ms | 12771 ms | 8188 ms | 4450 ms | 18 | ok, target ≤ 30 s met |
| Effigy 5000 ms query immediately after that refresh | 5000 ms | 2046 ms | 1813 ms | 287 ms | 18 | ok |
| Fixture cold `graph index` | unbounded | 166 ms | 48 ms | 200 ms | 12 | ok |
| Fixture warm, 5 runs | 5000 ms | p50 113 ms | 39 ms | 129 ms | 12 | ok |
| Fixture stale-incremental, all 7 Markdown files edited | 120000 ms | 247 ms | 83 ms | 277 ms | 12 | ok |
| Fixture 5000 ms query immediately after | 5000 ms | 113 ms | 40 ms | 126 ms | 12 | ok |

The `graph status` freshness state was `ready` with `stale_path_count` 0 before
each warm row, and the working-tree state is named per row because it changes
which freshness path runs: a clean tree at the indexed HEAD takes the git
skip-gate, a dirty tree takes the scan-state walk.

The roadmap's Chatterbox baseline (warm ≈ 10.7 s, every 5000 ms probe timing
out) is not reproduced here. That baseline was taken on the shared `effigy`
checkout while a concurrent `effigy graph index` ran; under this lane's
controlled conditions the warm query costs 1.6–2.0 s. The frozen budgets did
not need re-planning: the target the lane exists to hit is missed on the dirty
tree, which is the state an agent is actually in while working.

## Phase Attribution

Taken with temporary in-process instrumentation on the base binary, discarded
before the repair. Both traces are single runs on the conditions above.

### Warm, current index, clean tree (1408 ms in process)

| Phase | Cost | Share |
| --- | --- | --- |
| store open + freshness (git skip-gate) | 30 ms | 2% |
| **documentation profile compile** | **646 ms** | **46%** |
| scope build (`kind_for` per document) | 319 ms | 23% |
| scope relations (`list_edges` 211 ms, `list_references` 75 ms) | 308 ms | 22% |
| scope `list_symbols` + `list_files` | 60 ms | 4% |
| rank (term hits 19 ms, seed reads 10 ms, score/traverse 11 ms) | 41 ms | 3% |
| select + diagnostics | 2 ms | 0% |

### Stale-incremental, 50 Markdown files (13.6 s in process)

| Phase | Cost |
| --- | --- |
| freshness scan 1 (walk + profile compile 655 ms) | 705 ms |
| freshness scan 2, after an uncontended lock acquire (profile compile 610 ms) | 660 ms |
| rebuild: profile compile | 594 ms |
| rebuild: demote typed relations | 368 ms |
| rebuild: repo walk | 20 ms |
| rebuild: per-file extraction and storage (50 files) | 4304 ms |
| rebuild: resolve typed relations | 435 ms |
| rebuild: whole-corpus `graph_search` rebuild | 4347 ms |
| freshness scan 3, post-refresh (profile compile 605 ms) | 652 ms |
| query: profile compile | 596 ms |
| query: scope + rank + select | 713 ms |

Five profile compiles at ~600 ms each account for 3.06 s, 22% of the stale
query, and one compile accounts for 46% of the warm query.

### Root cause

`compile_docs_profile` ends in `reject_kind_overlaps`, which glob-matches every
in-scope Markdown document against every configured kind; `collect_scope` then
calls `kind_for` per document, which glob-matches again. `glob_matches`
compiled a fresh `GlobMatcher` on every call. On this profile that is 2425
documents times 12 kinds times their include/exclude patterns, per compile —
tens of thousands of glob compilations for 17 distinct patterns.

## Repair

One behavioural change plus one diagnostic addition, both inside the existing
refresh path, lock, health snapshot, and typed timeout.

- `crates/effigy-codegraph/src/docs_profile.rs`: compile each distinct
  normalized pattern at most once per process behind an `RwLock`-guarded map.
  Cached verdicts are the verdicts the uncached path produced, including the
  "invalid pattern never matches" verdict cached as `None`. No pattern,
  ordering, fingerprint, or overlap rule changed.
- `crates/effigy-codegraph/src/phase.rs` (new): the graph records its current
  phase, and file progress where the phase counts files, into process-global
  atomics. Advisory only; nothing branches on it.
- `refresh.rs`, `index.rs`, `docs_context/mod.rs`: record phase transitions.
  `refresh-lock-wait` is recorded only on a real wait, so an uncontended
  acquire cannot overwrite the caller's phase.
- `src/runner/graph_time_budget.rs`: `effigy.graph.timeout.v1` gains an
  additive `phase` block and one recovery line naming it. The detached worker
  is still running when the reporting thread reads it, which is why the
  recorder is process-global rather than thread-local; the runner clears the
  record before each bounded run so a timeout cannot name a previous command's
  phase, and reports JSON null when the bound expired before graph work began.
  Schema id, schema version, and every existing field are unchanged.

Diff: 11 files (5 source, 3 test, 3 documentation).

Deliberately not repaired, because no reproduced row shows them missing a
frozen budget: the whole-corpus `graph_search` rebuild on any change (4.3 s of
the stale path), per-file autocommit inserts (4.3 s of the stale path), the
three redundant freshness walks in one stale query, and `collect_scope` loading
all 208k edges and 42k symbols to use the doc-relation subset. Each is
attributed above and is a candidate for a future lane.

## After Table

| Condition | Budget | Wall | User | Sys | Load | Outcome |
| --- | --- | --- | --- | --- | --- | --- |
| Effigy cold `graph index`, empty `.effigy/graph` | unbounded | 147662 ms | 58880 ms | 83466 ms | 7.3 | ok, **19% faster than the 182388 ms baseline** |
| Effigy warm, current index, clean tree, 5 runs | 5000 ms | p50 602 ms (594/595/602/603/619) | ~394 ms | ~261 ms | 12.0 | ok, **3.2x faster** |
| Effigy warm, current index, dirty tree, 5 runs | 5000 ms | p50 682 ms (659/671/682/685/710) | ~438 ms | ~296 ms | 12 | ok, **3.0x faster, target met** |
| Effigy stale-incremental, 50 tracked Markdown files edited | 120000 ms | 10242 ms | 5157 ms | 4624 ms | 12 | ok |
| Effigy 5000 ms query immediately after that refresh | 5000 ms | 646 ms | 417 ms | 286 ms | 12 | ok, **3.2x faster** |
| Fixture cold `graph index` | unbounded | 128 ms | 44 ms | 158 ms | 18 | ok |
| Fixture warm, 5 runs | 5000 ms | p50 98 ms | 34 ms | 121 ms | 18 | ok |
| Fixture stale-incremental, all 7 Markdown files edited | 120000 ms | 214 ms | 75 ms | 270 ms | 18 | ok |
| Fixture 5000 ms query immediately after | 5000 ms | 96 ms | 35 ms | 118 ms | 18 | ok |

Every frozen budget holds, on both targets, with margin.

### Fixture benchmark cases, warm, 5000 ms

| Case query | Wall |
| --- | --- |
| `widget calibrator tolerance band` (charter authority, authority gate) | 95 ms |
| `escalation rota paging order` (current over retired) | 93 ms |
| `recall of 1998` (historical direct) | 98 ms |
| `quokka marmalade trombone` (no match) | 101 ms |
| `widget calibrator tolerance band --max-hops 1 --max-sections 12` (relation follow-up) | 106 ms |

## Benchmark

`effigy perf:docs-context-benchmark`, matrix unedited
(`git diff --stat HEAD -- scripts/benchmark-docs-context.rhai` is empty):

- before: `all predeclared docs-context expectations held`, 18833 ms
- after: `all predeclared docs-context expectations held`, 12105 ms

The freeze history in `scripts/benchmark-docs-context.rhai` names this run as a
replay at `696200af3`, not a freeze: no case, query, expected source, rival,
rank bound, dimension, or pass criterion changed. The matrix remains the
11-case freeze at `ff95f6a4c`.

## Pilot Replay

Effigy-local only, `--max-sections 3 --max-bytes 6000`, `EFFIGY_GRAPH_TIMEOUT_MS=5000`,
three runs each. Results are identical before and after; only latency moved.

| Question | Wall before | Wall after | Results |
| --- | --- | --- | --- |
| K4 `catalog_tasks` | 1716 / 1767 / 1887 ms | 696 / 685 / 638 ms | 3 |
| K4 as prose, `Which JSON field lists Effigy tasks` | 2004 / 1917 / 1903 ms | 666 / 682 / 684 ms | 3 |
| K5 `Does release execute publish a GitHub Release` | 1713 / 1723 / 1716 ms | 692 / 695 / 693 ms | 3 |
| no-match control | 1684 / 1635 / 1631 ms | 625 / 637 / 611 ms | **0** |

K4 `catalog_tasks`, both before and after:

1. `docs/research/source-hubs/002-catalog-pack-publication-source-map-v1.md#source-map-002-catalog-pack-publication`, lines 1-62, bytes 0-5009, research, authority 20, current
2. `docs/guides/071-catalog-service-authoring.md#add-a-new-catalog`, lines 161-170, bytes 4216-4586, guide, authority 50, current
3. `docs/guides/067-catalog-services-reference.md#catalog-layers`, lines 50-64, bytes 1181-1761, guide, authority 50, current

K5, both before and after:

1. `docs/roadmaps/g02/batch-cards/085-execute-demo-release-protocol.md#085-execute-demo-release-protocol`, lines 1-60, bytes 0-1621, ready-card, authority 60, historical
2. `docs/roadmaps/g02/batch-cards/177-implement-effigy-release-git-execute-follow-up-extraction.md#177-implement-effigy-release-git-execute-follow-up-extraction`, lines 1-45, bytes 0-1337, ready-card, authority 60, historical
3. `docs/roadmaps/g04/011-contract-promotion-and-closeout.md`, lines 1-40, bytes 0-1052, roadmap, authority 70, historical

Every row is a real Effigy section with an exact path and span. Nothing is
fabricated, nothing is cross-repository, and the no-match control returns an
empty report.

The no-match control literal is recorded here for replay. Recording it inside
`docs/logs/` — a profile root — gives its terms a non-zero document frequency
from this commit onward, exactly as the benchmark's fifth-freeze note warns.
The rows above were taken before this file existed. Do not re-run that literal
expecting an empty report; the fixture-owned `generic-no-match` case is the
durable empty-result proof.

### Open finding, not repaired in this lane

The pilot's expected K4 source is guide `026`'s task payload, and its expected
K5 source is guide `051`. Neither is returned at `--max-sections 3`. At
`--max-sections 32` guide `051` ranks 24 and guide `026` does not appear at
all. The cause is retrieval semantics, not latency: `query_terms` splits on
non-alphanumeric characters, so `catalog_tasks` never searches as one token and
becomes `catalog` (df 335) plus `tasks` (df 463) over 2425 scoped documents,
and the K5 prose question is carried by `release` (df 1205) and `execute`
(df 1034). Contract `041` ranks relevance first, and both queries are lexically
undiscriminating against this corpus.

Repairing it means changing tokenization or ranking, which spec `120` forbids
and reserves to Chatterbox. It is recorded here and left for planning. Spec
`120` oracle row 7 is satisfied — nothing fabricated, no wrong-repository
source, no-match empty — while card `1113`'s stricter acceptance line on the
expected sources is not, for the reason above.

The skill reference the pilot also named, `.agents/skills/effigy/references/json-envelope.md`,
is outside this repository's committed profile roots (`README.md`, `AGENTS.md`,
`CHANGELOG.md`, `PAPERCUTS.md`, `docs`), so it is correctly out of scope for
retrieval rather than missing from it.

## Validation

| Check | Result |
| --- | --- |
| `cargo test -p effigy-codegraph --lib` | 120 passed |
| `cargo test --lib docs_context_time_budget` | 7 passed |
| `effigy perf:docs-context-benchmark` | all predeclared expectations held |
| `effigy qa` | pass |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `git diff --check` | clean |
| `effigy graph affected` on the changed source | 100 affected files, 4 likely test files, `qa` selected |

Focused tests added:

- `cached_glob_matchers_stay_pattern_keyed_across_repeated_paths` — the cache
  keys on pattern, not on the first path it saw, and caches the invalid-pattern
  verdict.
- `every_reportable_phase_round_trips_and_is_named` — every reportable phase
  round-trips its code and appears in the closed name set.
- `refresh_and_query_record_a_readable_graph_phase` — a refresh and a query
  leave a readable phase whose progress never exceeds its own total.
- `timeout_detail_names_the_phase_the_bound_expired_in` — the timeout detail
  keeps `effigy.graph.timeout.v1` and version 1, keeps its health snapshot and
  existing recovery guidance, and its phase block is either JSON null or a
  known phase with well-formed progress. Both shapes are contractual: a bound
  can expire before the worker reaches graph work.
- `phase_description_reports_progress_only_when_the_phase_counts_items` — the
  recovery line renders progress for file-proportional phases and omits it
  otherwise.

The bounded runner clears the phase record before every run, so a timeout can
never name a phase left by an earlier command in the same process.

## Review Oracle

| Counterexample | Status |
| --- | --- |
| Code diff with no before-table naming the budget it addresses | falsified: the warm dirty-tree row misses the ≤ 2000 ms target and the phase attribution names the cost |
| A timing row missing an identity field, or taken under lock contention | falsified: identity above, no concurrent graph process, no lock holder |
| Warm Effigy query still times out at 5000 ms | falsified: p50 602 ms clean, 682 ms dirty |
| Stale-incremental still times out, or the following 5000 ms query fails | falsified: 10242 ms then 646 ms |
| Benchmark red or its matrix edited | falsified: green, `git diff` on the script is empty |
| Provenance, freshness identity, lock, health snapshot, or bounded-failure envelope changed or bypassed | falsified: replay rows byte-identical; the timeout block is additive |
| A default budget raised, or a new index/daemon/flag | falsified: no default, flag, or environment variable changed |
| K4/K5 replay returns a wrong or fabricated source; no-match returns a result | falsified: every row is a real Effigy section with exact span; no-match empty. The separate expected-source gap is recorded above as an open planning finding |
| Cold `graph index` regresses beyond 10% | falsified: 147662 ms against a 182388 ms baseline, 19% faster |

## Reserved Surfaces

Card `1113`, roadmap `g09.005`, spec `120`, contract `041`, `docs/logs/README.md`,
`docs/specs/README.md`, `docs/roadmaps/README.md`, and `docs/roadmaps/g09/README.md`
are left untouched for the coordinator to reconcile after merge.
