# Graph-Aware Scan Closeout

Date: 2026-05-25
Roadmap: `g08.008`
Batch card: `1036`

## Scope

Closed the graph-aware scan lane with focused test reruns, docs proof, and one
fresh benchmark pass against the current binary.

## What changed

- reran the focused graph and scan suites from one clean target dir
- reran docs checks for command references, JSON examples, index state, and
  next-action posture
- reran `perf:graph-agent-benchmark` and recorded both navigation and
  graph-aware scan proof
- closed the strict lane, batch card, and roadmap front doors

## Proof summary

Graph-backed navigation benchmark:

- fixture ownership and behavior queries still resolve correctly via graph
- exact-token lookup still stays `rg`-preferred
- live repos still resolve correctly:
  - Effigy shell-exit prompt owner:
    `src/runner/container_command/closeout.rs`
  - Underlay admin validation owner:
    `acme-client/src/commands/admin/validation-commands.ts`
  - decodelabs brains codebase hook owner:
    `legacy/directory/front/hooks/_nodes/HttpCodebase.php`

Graph-aware scan proof:

- `validation-gaps` fixture case returned the expected likely test file and
  task without inventing a finding
- `boundary-violations` stays clean when no rules are configured
- `god-files` and `attention-markers` only gain graph evidence when
  `--graph-context` is requested and the index is usable

Observed timings from the rerun:

- fixture graph queries: about `0.00s` to `0.01s`
- fixture scan proof: about `0.01s`
- Underlay live graph query: about `1.09s`
- decodelabs live graph query: about `0.89s`
- Effigy live graph query: about `8.62s`

## Accepted limits

- existing scans remain deterministic without an index; there is still no
  hidden auto-indexing
- graph enrichment is additive evidence, not a replacement for direct code
  inspection
- `dead-code` and `validation-gaps` remain advisory; they provide likely risk,
  not compiler-grade proof
- `boundary-violations` is the only new scan family that is strict-ready for
  future gates, and only when a repo declares explicit layer rules and leaves
  heuristic edges off
- graph-native scans can require graph facts while the top-level `graph`
  summary still reports `applied: false`; that field tracks additive
  enrichment, not general graph dependence
- Effigy live graph latency is still materially slower than raw `rg`

## Validation

- `cargo fmt --all -- --check`
- `CARGO_TARGET_DIR=/tmp/effigy-g08-1036 cargo test -p effigy-codegraph context_quality -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-g08-1036 cargo test -p effigy-builtin scan::tests -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-g08-1036 cargo test -p effigy graph_ -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-g08-1036 cargo test -p effigy run_manifest_task_builtin_scan_ -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-g08-1036 cargo build --bin effigy`
- `/tmp/effigy-g08-1036/debug/effigy perf:graph-agent-benchmark /tmp/effigy-g08-1036/debug/effigy`
- `effigy docs check links ...`
- `effigy docs check json-examples`
- `effigy docs check index`
- `effigy docs check next-action --policy vision`

## Vision Target Delta

- primary tags: `ROUTE`, `CONTRACT`, `OPERATE`, `MAINT`
- moved: filesystem-first scan preserved, additive graph readiness contract
  landed, existing scans gained optional graph evidence, and new graph-native
  scans now cover boundaries, likely dead code, and validation risk with
  fixture and live proof
- remains open: no active ready card; future work is planning-only
