# Test quality ratings

Owner: [test assurance](../test-assurance.md).

The test tables are rated at `ROUND4C_RATED_COMMIT`; the code table has not yet been rated.
Round 4 re-rated every row that changed, arrived or was recorded as an exception across the B1-B9
tranches, dropped the rows whose tests are gone, and left every untouched row's parts as they were
rated at `4026bd67e`. Locators and body hashes are refreshed for the rows that were re-read. The
round 4b close re-rated the 65 rows its consolidation touched, dropped the 36 rows whose test was
merged into a single entrypoint or is now a plain helper, and refreshed the rated commit. Round 4c
re-rated the final 39 below-floor rows; the updated summary reports zero scored tests below 7.

## Tables

- [Test summary](tests/SUMMARY.md)
- [T1 containers](tests/T1.csv), [T2 gateway/host/managed](tests/T2.csv), [T3 kernel/custody](tests/T3.csv)
- [T4 CLI/config/engine/UI](tests/T4.csv), [T5 other Rust](tests/T5.csv), [T6 Python harness](tests/T6.csv)
- [Code modules](code/README.md)

## Targets

Tom agreed to option B now: a test floor of 7, with evidenced merges and removals completed. Option C (floor 7 and mean 8.5) is a later target. The code target is TBD.

The agreed Round 4 target is met: every scored row is 7 or higher. The [summary](tests/SUMMARY.md)
records the current distribution, the non-scored helper and measurement entries, and the Round 4c
dispositions.

## Cost clarification (2026-11-12, from calibration)

- **C=0** is reserved for tests that sleep or wait on wall-clock time, run the full CLI binary end
  to end, push to real git remotes, or take more than about 5 s.
- Short child spawns (fake docker/nerdctl scripts, `sh -c printf`, local git init/clone, small
  fixture processes) are **C=1**.
- Pure in-process tests are **C=2**.
- A test whose only loss is cost because it must cross a real boundary (actual process, socket,
  FIFO, journal) to prove what it claims stays C=0 or 1. It isn't penalised further: its Value and
  Effectiveness carry it.

## Test rubric

# Effigy test-value rating, November 2026: charter

Every rating agent reads this first.

## Why
Tom, 2026-11-11: "have an agent go through all of the tests we have and give it a rating for effectiveness and usefulness; does it test something valuable? Then put all of those ratings in a table and work up from the bottom improving, removing or merging tests until we hit a reasonable score across the whole table." This sweep produces that table. Tom and the planner set the target score after seeing it. Nothing changes in this sweep.

## Hard constraints
- **Snapshot:** judge main at exactly `4026bd67e`. Your task checkout should be at that commit; if it isn't, read with `git show 4026bd67e:<path>`. Cite `path:line` at that commit.
- **Read only:** no source edits, commits, PRs or other tasks. Don't run test suites or builds. You may list tests (`rg`, reading `#[test]`, `fn test_`, `def test_`), read code, and use `git log`/`git blame`.
- **Write** only your own files under `/Volumes/Stash/scratch/effigy/test-rating-2026-11/ratings/`.

## Score: 0–10, the sum of four parts
| Part | Range | Question |
|---|---|---|
| **V**alue | 0–3 | Would a real regression in behaviour that users, consumers (CBS, inventors, Acowtancy/Underlay, DecodeLabs) or a safety contract (custody, redaction, fail-closed, cleanup) rely on make this test fail? 3 = core contract; 2 = real behaviour; 1 = minor or internal detail; 0 = nothing that matters. |
| **E**ffectiveness | 0–3 | Does it assert behaviour and outcomes rather than labels, incidental strings, implementation shape or the existence of a call? Would it catch the bug its name implies, with a deterministic oracle and no wall-clock luck? 3 = sharp; 0 = can't fail usefully, or asserts nothing meaningful. |
| **U**niqueness | 0–2 | 2 = nothing else proves this; 1 = partly overlaps; 0 = another test proves the same thing. |
| **C**ost | 0–2 | 2 = cheap (pure, fast, no process or fixture); 1 = moderate; 0 = heavy (real processes, full fixtures, more than about 5 s, or historically flaky). |

**Action:** `keep`, `improve` (say how), `merge` (into which test), or `remove` (say what's lost).

## Evidence rules
- Any `merge` or `remove`, and any U = 0, must name the overlapping test as `path:line` and state what is kept or lost. No evidence, no merge or remove.
- Any E ≤ 1 must quote the weak assertion briefly, as `path:line`.
- Mark inference as inference. Don't guess runtime: use C from the code shape (process spawn, fixtures, sleeps, timeouts).
- Tests that differ only in data (table rows, matrix cells) may share one row that lists every member name and states the count. Don't hide distinct behaviours in a group.

## Output
1. `ratings/<ID>.csv` with header `id,file_line,V,E,U,C,score,action,reason,evidence`:
   - `id` = `binary-or-crate::module::test_name`, or `py:file::Class::test` for Python;
   - `reason` is one line;
   - `evidence` = `path:line` references, separated by `;`.
2. `ratings/<ID>.md`, at most two pages:
   - the score distribution (count per score 0–10);
   - the 20 lowest-scoring tests with one line each;
   - families worth merging into tables;
   - expensive tests that buy little;
   - anything that looked untested but valuable, without inventing new tests;
   - coverage notes: total tests rated, how you enumerated them, and anything skipped with the reason.

Rate every test in your scope. Completeness matters more than prose.

## Code rubric

# Effigy product-code module rating: charter (draft; dispatch when rounds 1–3 close)

## Why
Tom, 2026-11-11, agreeing the planner's proposal ("Sounds spot on, let's do it"): rate Effigy's product code at module level the way the tests were rated. Tom sets a target from the table. The improvement loop is round 5, which runs only after round 4 has brought tests to floor 7.

## Unit
A module: one Rust source file, or a tight file group with one responsibility, such as `foo/mod.rs` with its private submodules. Product code only, under `src/` and `crates/*/src/`. Exclude tests, test-support and test-only modules, the private qualification harness, and generated code. Expect about 1,500–2,000 units.

## Score: 0–10, the sum of five parts
| Part | Range | Question |
|---|---|---|
| **O**wnership | 0–2 | Is this the single owner of its mechanism (2), or one of several places doing the same thing (0)? Name the duplicates. |
| **T**yped facts | 0–2 | Does it decode facts once into types and let callers decide (2), or re-derive them from strings, labels or debug text (0)? |
| **F**ailure | 0–2 | Does it fail closed where the contract requires, keep unknown distinct, and give errors that say what to do (2)? Or does it fail open or lose state (0)? |
| **P**roportion | 0–2 | Is its size and branching proportionate to its job (2), or has it piled up flags, special cases, retries and patch layers (0)? Use churn and `git log` as evidence. |
| **Pr**oof | 0–2 | How well do tests cover it? Use `/Volumes/Stash/scratch/effigy/test-rating-2026-11/ALL-ratings.csv`: are its tests at 7 or above and behaviour-level (2), weak (1), or absent (0)? |

**Action:** `keep`, `simplify` (say what), `consolidate` (with which module), `harden` (which failure path), `split`, or `delete` (dead or vestigial; say how you know).

## Objective columns (computed, not judged)
Lines of code, 30-day churn (commits), 90-day churn, and number of distinct fix commits touching the module.

## Evidence rules
- Any O = 0, `consolidate` or `delete` needs `path:line` for the duplicate, or proof of no callers.
- Any T or F of 1 or less needs `path:line` for the offending code.
- Mark inference as inference. Judge the code as it is at the frozen commit; the re-audit reports are context, not proof.

## Output
- `ratings/<ID>.csv` with header `module,files,loc,churn30,churn90,fixes,O,T,F,P,Pr,score,action,reason,evidence`.
- `ratings/<ID>.md`, at most two pages:
  - the score distribution;
  - the 20 lowest modules;
  - consolidation clusters (modules that should become one owner);
  - dead-code candidates;
  - coverage notes.

## Domains (same split as the test sweep)
| | Domain |
|---|---|
| C1 | containers |
| C2 | gateway, host and managed |
| C3 | kernel and custody |
| C4 | CLI, config, engine and UI |
| C5 | other crates and root |

There's no Python harness domain; the harness is out of scope as product code.

## Calibration
Run an independent stratified sample of at least 60 modules, with one frontier calibrator if the cheaper raters disagree by more than ±1. Design judgement is noisier than test judgement.

## Test results at the rated commit

- 7,701 table rows: 6,437 Rust and 1,264 Python. Thirty parent-invoked helper and child entrypoints are marked `helper` in the CSVs and excluded from scoring, leaving 7,671 scored tests.
- Distribution (scored tests, scores 4–10): 5, 7, 27, 1,617, 2,429, 2,797, 789. Mean 8.35; median 8; 0.2% scored 5 or lower and 0.5% scored 6 or lower.
- Table rows, including excluded helpers: T1 1,306; T2 623; T3 858; T4 1,793; T5 1,857; T6 1,264.
- The 422 rows whose tests no longer exist left the table at the round 4 close, and round 4b took 36 more (merged entrypoints and converted helpers) while 19 survivors arrived; six duplicate Python rows that named a test under a class that never declared it also left the T6 table. 39 scored rows now sit below the floor of 7 (T1 18, T2 8, T3 3, T4 6, T5 4, T6 0) instead of 870 at 6 or below before round 4.

An independent Claude Sonnet pass re-scored a stratified 42-test sample from scratch at `4026bd67e`:
- 23 of 42 scores match and 38 are within ±1;
- mean difference −0.14 and mean absolute difference 0.57.

Two biases roughly cancel:
- **Effectiveness is overclaimed** when a test's name promises more than its assertions check (net −6 across 16 rows).
- **Cost is too harsh** on short child spawns (net +6 across 8 rows).

T4 is mildly inflated (−0.86 on average). The true mean is probably about 7.9–8.0. The calibrator would also reverse 9 `keep` rows (3 to merge, 6 to improve) and adds no removals. Details: `calibration-summary.md`.

The round 4 close re-rated 865 rows (151 new, 365 changed, 349 B1–B9 exception rows) against the
clarified cost rule and recorded the new rated commit. The round 4b close re-rated the 65 rows its
consolidation touched: 57 were raised, 7 kept their score and 1 fell to 7 by the clarified cost
rule. Neither re-ran the independent sample; the calibration above still describes the original
sweep.

The drift tool hashes a whole Rust function item. It now skips raw strings, byte and C raw strings,
char literals, lifetimes, line comments and nested block comments, so a brace inside one of those no
longer ends an item early or swallows the following tests. It also forwards `--summary`, and the
rated-commit constant and expected row counts match the tables above.

## Update policy

At each milestone close and after each simplification round, the planner re-rates added and changed tests (and later modules) from the check output, removes dead rows, refreshes the summary, and records the new rated commit. Workers do not edit these tables in ordinary tasks.
