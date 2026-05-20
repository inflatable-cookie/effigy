# Graph Agent Adoption Closeout

Date: 2026-05-20
Roadmap: `g07.078`
Batch card: `1028`

## Scope

Closed the graph agent-adoption follow-through lane with rerun benchmark proof,
focused graph tests, and explicit residual limits.

## What changed

- reran `perf:graph-agent-benchmark` against a fresh binary built from the
  current checkout
- fixed the benchmark script so missing packet fields do not crash the task
  when an older binary is supplied
- reran focused graph-ranking and runner JSON-contract tests
- closed the strict lane and roadmap front doors

## Benchmark summary

Fixture-backed cases:

- split-owner edit query resolved via graph with the correct edit target and
  bounded test hints
- exact token lookup stayed `rg`-preferred
- redirect ownership resolved via graph
- migration-validation ownership resolved via graph

Live repos:

- Effigy shell-exit prompt query resolved via graph to
  `src/runner/container_command/closeout.rs`
- Underlay admin validation query resolved via graph to
  `acme-client/src/commands/admin/validation-commands.ts`
- decodelabs brains codebase-hook query resolved via graph to
  `legacy/directory/front/hooks/_nodes/HttpCodebase.php`

Observed timings from the rerun:

- fixtures: about `0.01s`
- Underlay live case: about `1.19s`
- decodelabs live case: about `0.94s`
- Effigy live case: about `10.32s`

## Accepted limits

- graph does not replace exact-token `rg`
- graph query latency on the Effigy repo is still materially higher than raw
  `rg`, even when the answer quality is better
- likely tests remain bounded candidates, not exhaustiveness proof
- index freshness still depends on explicit `graph index` or `graph watch`
  rather than hidden automatic mutation

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-graph-adoption-closeout cargo build --bin effigy`
- `effigy perf:graph-agent-benchmark /tmp/effigy-graph-adoption-closeout/debug/effigy`
- `CARGO_TARGET_DIR=/tmp/effigy-graph-adoption-closeout cargo test -p effigy-codegraph context_quality -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-graph-adoption-closeout cargo test -p effigy graph_ -- --nocapture`
- `effigy docs check links ...`
- `effigy docs check index`

## Vision Target Delta

- primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- moved: graph usefulness audit -> cross-repo trust model, behavior ranking,
  edit/test packet proof, benchmark evidence, and balanced agent guidance
- remains open: no active ready card; future work is planning-only
