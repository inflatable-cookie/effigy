# Code ratings: `a9b3213f9` baseline, round 5 re-rate

The tables `C1.csv`–`C5.csv` hold 799 product-code modules (C1 196, C2 74,
C3 105, C4 182, C5 242). They were rated at commit `a9b3213f9` and re-rated at
`e458614d8` after round 5's splits, so every row's `body_sha` matches the tree
it was judged on. Columns are `id,files,loc,given,corrected,source,action,reason,body_sha`;
scores stay frozen between re-rates: a re-rate writes new tables, never edits to
the old snapshot. Column semantics are in `README.md`.

## Round 5 re-rate (effigy#958)

Round 5 landed as splits and repairs, so the table was re-rated rather than
patched: at `e458614d8` the frozen table reported 95 new, 150 changed and 2 gone
rows (30.8% stale). Every drifted row was re-read at current main with the same
rubric and the same evidence standard: each of the 244 re-scored and new rows
carries a one-line `reason` whose text ends in a concrete `path:line` citation
for the mechanism it describes, and the 555 rows whose files did not change keep
their frozen reason and `body_sha` verbatim.

| Work | Count |
|---|---|
| Rows re-scored because their text changed | 150 (C1 55, C2 19, C3 10, C4 37, C5 29) |
| New rows for modules with no row | 94 (C1 42, C2 11, C3 8, C4 3, C5 30) |
| Gone rows removed | 2 (`runner::deploy_command`, `runner::script_command`) |
| Frozen rows re-verified unchanged and not re-scored | 555 |

The 94 new rows are the round-5 children, each named under its former parent's
domain: `deploy_command/` and `script_command/` (C5), `container_command/lifecycle/`,
`container_command/support/`, `container_runtime_prep/` and `provisioning/` (C1),
`host_process/` (C2), the kernel fault harness `fault*.rs` and `faults/` (C3), the
built-in test-planning facade and the `src/runner/execute` surface (C4), and
`task_secrets.rs` with `task_engine/tail.rs` (C5). Four files (`execute.rs` and
three one-line shims) are rated as the one `src/runner/execute` row, the same
fold the frozen table used for `host_process/waits.rs`.

Two frozen rows are gone because their files were replaced by directories. They
are not deletions of behaviour:

- `runner::deploy_command` (given 6, corrected 4, 4,175 lines) **split into**
  `src/runner/deploy_command` (the surviving dispatch/config/report/text files)
  and its 18 children: `code_ref`, `export`, `inputs`, `model/{derive,manifest,merge,mod,production,vocabulary}`,
  `plan`, `provider/{context,descriptor,mod,phase,report,source}`, `refuse`, `status`.
- `runner::script_command` (given 6, corrected 4, 1,509 lines) **split into**
  `src/runner/script_command` (the surviving host file) and its 8 children:
  `feature_dispatch/{container,deploy,docs,inspect,mod,state,surface}` and `nested_error`.

### Corrected distribution after the re-rate

`given` is the raw score; `corrected` applies the calibration offset below. Every
re-rated row is on the offset (`source: offset`); the 32 rows that were
blind-calibrated at `a9b3213f9` keep their blind score where their text did not
change.

- **Mean:** 7.89 corrected (given 8.96) — unchanged from the `a9b3213f9` mean,
  because the splits moved score out of the large files into small ones rather
  than changing how the code reads.
- **Scores:**

| Score | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
|---|---|---|---|---|---|---|---|
| Modules | 1 | 5 | 66 | 169 | 325 | 231 | 2 |

- **5 or below:** 6 modules (1%), about 11k lines — down from 15 modules and 24k lines.
- **6 or below:** 72 modules (9%), about 70k lines — down from 79 and 83k.
- **7 or below:** 241 modules (30%), about 192k lines — up from 216 and 174k,
  because the split children carry the same judgements at a smaller size.

Lowest after the re-rate: `exec/implementation` (4); `exec/colima_runtime`,
`exec_command/transport`, `builtin/test/execution`, `effigy-ui::lib`,
`effigy-ui::steps` (5).

### The eight round-5 targets, before and after

| Target | Before (given / corrected / loc) | After |
|---|---|---|
| `runner::deploy_command` | 6 / 4 / 4,175 | split: `src/runner/deploy_command` 9 / 8 / 859, children 8–10 given |
| `workspace_command/provisioning.rs` | 8 / 4 / 3,512 | 9 / 8 / 31 (nine owners behind it) |
| `container_command/support.rs` | 6 / 4 / 1,591 | 9 / 8 / 76 (ten owners behind it) |
| `runner::script_command` | 6 / 4 / 1,509 | split: `src/runner/script_command` 9 / 8 / 372, children 8–9 given |
| `container_command/lifecycle.rs` | 6 / 5 / 4,120 | 9 / 8 / 52 (eleven owners behind it) |
| `host_process` | 9 / 5 / 2,222 | 9 / 7 / 4,247 (shared types plus six owners) |
| `container_runtime_prep/mod.rs` | 7 / 5 / 1,939 | 9 / 8 / 83 (ten owners behind it) |
| `execute/pipeline/standard.rs` | 8 / 5 / 1,643 | 9 / 8 / 1,157 |

No split child scores 5 or below: the parents' low scores were proportion, not
behaviour, and the children keep the behaviour at a size that matches it.

### Modules still at 5 or below

| Module | Given | Corrected | Loc | Why |
|---|---|---|---|---|
| `crates/effigy-containers/src/exec/implementation.rs` | 6 | 4 | 3,976 | four jobs in one file; split outstanding |
| `crates/effigy-containers/src/exec/colima_runtime.rs` | 7 | 5 | 2,042 | ensure + observe + recover + reset in one module |
| `src/runner/exec_command/transport.rs` | 7 | 5 | 1,756 | shares bounded capture and argv duties with `bounded.rs` and `transport/colima.rs` |
| `crates/effigy-builtin/src/test/execution.rs` | 7 | 5 | 1,540 | two runners plus per-suite rendering in one module |
| `crates/effigy-ui/src/lib.rs` | 6 | 5 | 35 | facade with no mechanism of its own |
| `crates/effigy-ui/src/steps.rs` | 7 | 5 | 1,910 | five mechanisms plus the live window |

## Calibration (why the scores are corrected)

Independent Claude Sonnet sub-raters re-scored 60 modules blind at `a9b3213f9`
(12 per domain, including the 5 largest by churn × size):

| Group | Mean difference (blind − given) |
|---|---|
| C1 | −1.50 |
| C2 | −1.67 |
| C3 | −1.58 |
| C4 | −1.08 |
| C5 | −1.92 |
| Files of 1,500+ lines | −1.92 |
| Smaller files | −1.29 |
| Overall (mean absolute difference 1.62) | −1.55 |

Deepseek was lenient mainly on Proportion (size and patch layers) and Typed facts.
Correction: sampled modules take their blind score; all others are offset by −1.9
(1,500+ lines) or −1.3 (smaller). The round-5 re-rate applies the same offset to
every re-scored row and leaves the blind rows alone, so the ranking within each
domain is preserved and the planner's blind check can still adjust the offsets.

## Distribution at `a9b3213f9` (the frozen snapshot)

- **Mean:** 7.89, down from a given 8.96.
- **Scores:** 4 × 4, 5 × 11, 6 × 64, 7 × 137, 8 × 260, 9 × 229, 10 × 2.
- **5 or below:** 15 modules (2%), about 24k lines.
- **6 or below:** 79 modules (11%), about 83k lines.
- **7 or below:** 216 modules (31%), about 174k lines.

Lowest at the snapshot: deploy_command, workspace provisioning, container support,
script_command (4); container lifecycle, host_process, host_process::kernel_run,
container_runtime_prep, execute/pipeline/standard (5).

## Concrete defects the calibration found: current state

Verified against the tree during the round-5 re-rate, with `file:line` in the
row's `reason` where the module was re-scored:

- `inline_workspace.rs`: **fixed** — `resolve_mount_source` resolves the declared
  source against the canonical repo root, lexically first and then through the
  filesystem, so `../shared` and a symlinked ancestor both refuse.
- `container_runtime_prep/mod.rs`: **split** — the module is now a documented
  surface; the runtime-data widening survives by design for container access and
  lives in `host_mounts::widen_runtime_data_entry`, bounded to the runtime data
  root, with every failure returned.
- `environment/reconcile.rs`: **fixed** — an admission failure keeps both the
  refusal and the full settlement (`EnvError::AdmissionCleanup`), including
  unresolved effects and recovery commands.
- `container_command/lifecycle.rs`: **moved out with the split** — no
  error-message-prefix classification remains under `lifecycle/`.
- `host_process`: **fixed** — lifecycle status and stop state are typed
  (`HostProcessState`, `HostStopState`), and a corrupt journal stays `Unknown`
  rather than being dropped.
- `managed::run_spec`: **fixed** — the delivered environment is a typed field
  beside the command (`RenderedRun`), so nothing re-parses the rendered shell.
- `managed_run::engine`: **fixed** — `pending_stop` refuses an unreadable stop
  mailbox instead of failing open.
- `execute/pipeline`: **partly fixed** — no `contains("exit=")` parse remains
  under `src/runner/execute/**`; secret resolution and redaction are owned by
  `task_secrets`, and delivered-environment resolution is one method on
  `host_alias`. The standard/managed/pipeline files still share route and
  secret-access decisions.
- Duplicate helpers: **fixed** — `parse_mount_parts`/`looks_like_bind_mount_source`
  have one owner (`mount_spec`), `shell_quote` one (`effigy_core::shell`), and
  digest hex one (`effigy_core::git_source::sha256_hex`, `hex::encode`).

## Module identity (how `id` and `files` were normalised)

Each frozen rating row (a mix of Rust paths such as `runner::deploy_command` and
file paths) became a stable `id` plus the exact file set it covered at
`a9b3213f9`. Single-file rows list that file; directory modules list the facade
`.rs` (when the rater counted it) plus the group's files, minus files claimed by
a nested rated row and minus test-only files (`tests/` trees, `tests.rs`,
`test_support`). Every row's file count and every group row's line total were
verified arithmetically against the frozen `files`/`loc` columns.

Deliberate exceptions, all verified by exact file-count and line-total match:

- Three rated overlaps are preserved as rated: the `events` row includes its
  nested `key_command` row's four product files; `test/render` shares
  `results.rs` with the `test/render/results` row; `src/runner/execute/pipeline`
  covers all five files including the separately-rowed `managed.rs` and
  `standard.rs` trees. Drift in a shared file flags every row that rated it.
- Two frozen `loc` values do not equal their whole-file sums and are copied
  verbatim: `test/planning/resolve` (rated 1140, files sum to 1103) and
  `src/runner/execute/pipeline` (rated 578; only the entry files sum near it).
  Drift uses `body_sha`, never `loc`. Re-rated rows carry their current line
  counts instead, computed with the same nested-row exclusion.
- Rater gaps (product files at `a9b3213f9` with no row): `test/planning.rs`,
  `test/planning/runnable/cargo_env.rs`, `arg_parser.rs` is folded into its group
  row, and the `waits.rs` re-export shim is folded into the `multiprocess/mod.rs`
  row. The first two are rated now.

## Target options (Tom decides)

| Option | Target | Work |
|---|---|---|
| A, floor 6 (recommended first) | No module below 6, and every concrete defect above fixed | 6 modules (about 11k lines) remain below 6 after round 5, plus the outstanding splits |
| B, floor 7 | No module below 7 | 72 modules (about 70k lines); a long programme |
| C | Mean 8.5 | Most of the codebase |

Round 5 works bottom-up, with smaller batches than for tests, and needs native
proof for runtime paths. It runs after round 4 has brought tests to floor 7.

## Drift: the work list for the next re-rate

Before round 5 the tables reported 95 new, 150 changed and 2 gone rows since
`a9b3213f9` (30.8% stale). After the re-rate at `e458614d8`:

```text
code quality: 799 rows, 0 new, 0 changed, 0 gone, 0.0% stale since a9b3213f9
```

`effigy qa:quality:code` compares each row's `body_sha` with the working tree, so
new drift means a row's files changed after the re-rate and needs a fresh rating.
Split groups are reported as `split: re-rate` when new files sit under a gone
row's path.
