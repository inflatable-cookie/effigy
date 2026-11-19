# Test and code quality ratings: frozen review artifact

Status: closed review artifact; no longer maintained. Owner: [test assurance](test-assurance.md).

The November 2026 rating sweep ran to completion and set the evidence the
pruning waves used. It is finished: the per-case and per-module tables are no
longer kept in the working tree, scores, locators and body hashes are no longer
refreshed, and no product path, selector or gate reads them. A later focused
audit that needs the rows reads them from history with
`git log --oneline -- docs/knowledge/quality` and
`git show <commit>:docs/knowledge/quality/tests/T1.csv`; the tables were last
rewritten at the commits their own summaries name for tests
(`7a502af038b7fbc4889d6484ea78deb25c7daac5`) and for code modules (`a9b3213f9`,
re-rated at `e458614d8` and after the floor-7 tranche, effigy#982).

Superseded by this: the in-tree tables (`README.md`, `tests/SUMMARY.md`,
`tests/T1.csv`–`T6.csv`, `code/README.md`, `code/SUMMARY.md`,
`code/C1.csv`–`C5.csv`, 8,692 lines), `scripts/quality-table.py` and its
`qa:quality:tests`, `qa:quality:code` and `test:quality:table` selectors.

## Method

A test row scored four parts out of 10: **value** (0–3, would a real regression
its users rely on fail it), **effectiveness** (0–3, behaviour and a
deterministic oracle rather than labels or implementation shape),
**uniqueness** (0–2, is it the only proof), and **cost** (0–2; 0 for
wall-clock waits, full-CLI runs, real remotes or more than about 5 s; 1 for
short child spawns on fakes; 2 for pure in-process). Any merge, removal or
uniqueness 0 named the overlapping case, and any effectiveness of 1 or less
quoted the weak assertion.

A code row scored five parts: **ownership** (one owner or duplicated),
**typed facts** (decoded once or re-derived from strings), **failure** (fails
closed, unknown stays unknown), **proportion** (size and branching fit the job),
and **proof** (its tests are behaviour-level). Locator, churn and module shape
were recorded as computed columns, not judged.

## Results

Tests, 7,700 rows (6,436 Rust, 1,264 Python): 41 parent-invoked helpers and one
measurement utility excluded, leaving 7,659 scored. Scored distribution
7 × 1,639, 8 × 2,434, 9 × 2,797, 10 × 789; mean 8.36, median 8; nothing below
the agreed floor of 7. Per domain (rows / scored / mean): T1 containers
1,305 / 1,304 / 7.87; T2 gateway, host and managed 623 / 617 / 8.37; T3 kernel
and custody 858 / 835 / 8.60; T4 CLI, config, engine and UI 1,793 / 1,788 /
7.84; T5 other Rust 1,857 / 1,853 / 8.70; T6 Python harness 1,264 / 1,262 /
8.92.

Code modules, 815 rows (C1 200, C2 73, C3 110, C4 189, C5 243), mean 8.10
corrected against 9.10 given; corrected 7 × 167, 8 × 403, 9 × 243, 10 × 2, and
no module at 6 or below.

An independent blind sample of 42 test rows matched 23 scores exactly and 38
within ±1 (mean difference −0.14, mean absolute difference 0.57), with
effectiveness overclaimed on 16 rows and cost too harsh on 8; the true test
mean is probably 7.9–8.0. A blind sample of 12 code rows landed a mean +0.17
above the rated score, so the −1 correction held.

## Policy

The floor is met and the sweep is closed. Milestone scoring and per-file score,
locator and body-hash refreshes are retired (operator ruling, v0.16 lane,
2026-11-19): they produced review churn without a consumer. A pruning wave now
justifies itself by the surviving equivalent check and a failing control for
risk-bearing cases, per the rules in [test assurance](test-assurance.md), not by
a score. New widespread rating work needs a fresh operator request.
