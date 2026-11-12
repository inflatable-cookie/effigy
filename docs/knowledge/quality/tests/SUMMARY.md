# Test ratings at `7a502af038b7fbc4889d6484ea78deb25c7daac5`

The tables contain 7,700 rows: 6,436 Rust and 1,264 Python tests. Forty parent-invoked helper and
child entrypoints plus one measurement utility are marked in `excluded`, leaving 7,659 scored
tests.

## Distribution (scores 0–10)

| Score | 0–6 | 7 | 8 | 9 | 10 |
|---|---:|---:|---:|---:|---:|
| Tests | 0 | 1,639 | 2,434 | 2,797 | 789 |

Mean 8.36; median 8. There are no scored tests below the agreed floor of 7.

## Per-domain results

| Domain | Table rows | Scored tests | Mean | Score ≤ 6 |
|---|---:|---:|---:|---:|
| T1 containers | 1,305 | 1,304 | 7.87 | 0 |
| T2 gateway/host/managed | 623 | 617 | 8.37 | 0 |
| T3 kernel/custody | 858 | 835 | 8.60 | 0 |
| T4 CLI/config/engine/UI | 1,793 | 1,788 | 7.84 | 0 |
| T5 other Rust | 1,857 | 1,853 | 8.70 | 0 |
| T6 Python harness | 1,264 | 1,262 | 8.92 | 0 |

## Round 4c close

The final 39 below-floor rows were re-read against their assertions. Twenty-seven behavior tests
were re-rated at 7 or higher. Ten fixture or child entrypoints were removed from the scored tables
with their parent test cited as the covering route. One duplicate pending-progress test was folded
into the retained inherited-writer test, which checks nonzero lower bounds for both partial streams
and that reads cover retained bytes. The anchor spawn timing utility remains available as an
ignored measurement tool and is excluded from scoring; it has no behavioral threshold. Its table
row is removed from scored tests.

Presentation rows remain attached to the production path they verify: fake-clock reconciliation
and restart-policy progress frames, doctor color styling, and typed general-help rendering. Their
assertions pin the exact operator-visible output. Readiness, refusal, identity, route-retry,
workspace path, bootstrap compatibility, and performance rows retain their distinct outcomes.

The baseline census and previous-round history are recorded in the earlier table revision. This
summary reports the current ratings only; it is not a fresh test census or a whole-suite QA result.
