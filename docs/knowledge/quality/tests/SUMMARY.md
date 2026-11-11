# Test ratings at `4026bd67eae1bdc51dad77bc9792795502e817f8`

The tables contain 7,993 rows: 6,736 Rust and 1,257 Python tests. Thirty ignored helper entrypoints are marked `helper entrypoint` and excluded from scoring, leaving 7,963 scored tests.

## Distribution (scores 0–10)

| Score | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Tests | 4 | 3 | 8 | 129 | 726 | 1,602 | 2,299 | 2,468 | 724 |

Mean 8.05; median 8. 1.8% scored 5 or lower, 10.9% scored 6 or lower, and 31% scored 7 or lower.

## Per-domain results

| Domain | Table rows | Scored tests | Mean | Score ≤ 6 |
|---|---:|---:|---:|---:|
| T1 containers | 1,464 | 1,463 | 7.06 | 428 |
| T2 gateway/host/managed | 633 | 631 | 8.14 | 60 |
| T3 kernel/custody | 870 | 849 | 8.54 | 10 |
| T4 CLI/config/engine/UI | 1,847 | 1,845 | 7.46 | 320 |
| T5 other Rust | 1,922 | 1,920 | 8.60 | 41 |
| T6 Python harness | 1,257 | 1,255 | 8.87 | 11 |

Calibration: an independent 42-test sample matched 23 exactly and 38 within ±1; mean difference −0.14 and mean absolute difference 0.57.
