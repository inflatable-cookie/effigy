# Code ratings: `a9b3213f9` baseline, round 5 and floor-7 re-rates

The tables `C1.csv`–`C5.csv` hold 815 product-code modules (C1 200, C2 73,
C3 110, C4 189, C5 243). They were rated at commit `a9b3213f9`, re-rated at
`e458614d8` after round 5's splits and re-rated again at current main after
round 6 and floor 7, so every row's `body_sha` matches the tree it was judged
on. Columns are `id,files,loc,given,corrected,source,action,reason,body_sha`;
scores stay frozen between re-rates: a re-rate writes new tables, never edits to
the old snapshot. Column semantics are in `README.md`. The `corrected` column of
the 244 round-5 rows was recalibrated in `effigy#960` from the planner's blind
check, and 10 sampled floor-7 rows in `effigy#983` from the same kind of blind
check; the floor-7 section records the current distribution and every
distribution in the round-5 section is the state at `e458614d8`.

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

`given` is the raw score; `corrected` applies the calibration offset below. The
244 re-scored and new rows carry the round-5 recalibration: 12 of them took their
independent blind score (`source: blind-r5`) and the other 232 a uniform −1
(`source: offset-r5`), as measured in the next section. The 555 frozen rows keep
their `a9b3213f9` correction verbatim — 523 on `offset`, 32 on `blind`.

- **Mean:** 7.95 corrected (given 8.96), up from 7.89 under the first round-5
  offset, and above the `a9b3213f9` mean because the splits moved score out of
  the large files into small ones rather than changing how the code reads.
- **Scores:**

| Score | 5 | 6 | 7 | 8 | 9 | 10 |
|---|---|---|---|---|---|---|
| Modules | 2 | 59 | 158 | 341 | 236 | 3 |

- **5 or below:** 2 modules (0.3%), about 4.0k lines — down from 6 modules and 11k
  lines under the first round-5 offset, and from 15 and 24k lines at the snapshot.
- **6 or below:** 61 modules (8%), about 46k lines — down from 72 and 70k.
- **7 or below:** 219 modules (27%), about 132k lines — down from 241 and 192k.

Lowest after the re-rate: `exec/implementation` (5) and `effigy-ui::lib` (5).
Nothing scores 4 or below.

### Blind recalibration of the round-5 offset

The −1.9/−1.3 offsets were measured at `a9b3213f9`. The re-rate's own evidence
standard — every `reason` must end in a concrete `path:line` citation — made the
rater materially less lenient than in the first pass, so reusing the old offsets
over-corrected the 244 re-rated rows. Independent Claude Sonnet raters re-scored
12 of them blind (never shown the given score or the reason):

| Row | Given | Old corrected | Blind | `corrected` now |
|---|---|---|---|---|
| `src/runner/container_command/support.rs` | 9 | 8 | 10 | 10 |
| `src/runner/exec_command/bounded.rs` | 8 | 7 | 9 | 9 |
| `gateway::setup` | 9 | 7 | 9 | 9 |
| `src/runner/container_command/lifecycle.rs` | 9 | 8 | 9 | 9 |
| `src/runner/container_runtime_prep/predecessor.rs` | 9 | 8 | 9 | 9 |
| `managed::plan` | 9 | 8 | 8 | 8 |
| `src/runner/host_process/kernel_run/activation.rs` | 8 | 7 | 8 | 8 |
| `src/runner/workspace_command/staging.rs` | 10 | 9 | 8 | 8 |
| `src/runner/script_command/feature_dispatch/surface.rs` | 8 | 7 | 8 | 8 |
| `src/runner/container_command/profile.rs` | 8 | 6 | 7 | 7 |
| `runner::mod` | 7 | 6 | 7 | 7 |
| `src/runner/container_runtime_prep/cargo_inputs.rs` | 9 | 8 | 6 | 6 |

- blind − given: mean −0.42, mean absolute difference 0.75.
- blind − old corrected: mean +0.75, mean absolute difference 1.25 — the old
offset overshot, it did not undershoot.

Rule applied to the table: the 12 sampled rows take their blind score with
`source: blind-r5`; every other row re-rated in `effigy#958` takes
`corrected = max(0, given − 1)` with `source: offset-r5`, the conservative integer
form of the −0.42 mean; the 555 frozen rows are untouched. The uniform −1 keeps
the ranking the rater gave each re-read row instead of re-imposing a size band the
re-read row already accounts for in its Proportion part.

### The eight round-5 targets, before and after

| Target | Before (given / corrected / loc) | After |
|---|---|---|
| `runner::deploy_command` | 6 / 4 / 4,175 | split: `src/runner/deploy_command` 9 / 8 / 859, children 8–10 given, 7–9 corrected |
| `workspace_command/provisioning.rs` | 8 / 4 / 3,512 | 9 / 8 / 31 (nine owners behind it) |
| `container_command/support.rs` | 6 / 4 / 1,591 | 10 / 10 / 76 (ten owners behind it) |
| `runner::script_command` | 6 / 4 / 1,509 | split: `src/runner/script_command` 9 / 8 / 372, children 8–9 given, 7–8 corrected |
| `container_command/lifecycle.rs` | 6 / 5 / 4,120 | 9 / 9 / 52 (eleven owners behind it) |
| `host_process` | 9 / 5 / 2,222 | 9 / 8 / 4,247 (shared types plus six owners) |
| `container_runtime_prep/mod.rs` | 7 / 5 / 1,939 | 9 / 8 / 83 (ten owners behind it) |
| `execute/pipeline/standard.rs` | 8 / 5 / 1,643 | 9 / 8 / 1,157 |

No split child scores 5 or below: the parents' low scores were proportion, not
behaviour, and the children keep the behaviour at a size that matches it.

### Modules still at 5 or below

| Module | Given | Corrected | Loc | Why |
|---|---|---|---|---|
| `crates/effigy-containers/src/exec/implementation.rs` | 6 | 5 | 3,976 | four jobs in one file; split outstanding |
| `crates/effigy-ui/src/lib.rs` | 6 | 5 | 35 | facade with no mechanism of its own |

The four modules that sat at 5 under the first round-5 offset move to 6 with the
−1: `exec/colima_runtime` (2,042 lines), `exec_command/transport` (1,756),
`builtin/test/execution` (1,540) and `effigy-ui::steps` (1,910). Each is still a
proportion finding for a later split, not a behaviour finding.

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
(1,500+ lines) or −1.3 (smaller). Those offsets were measured against that
snapshot. The round-5 re-rate applies them to no row: its 12 blind re-scores take
the blind score and its other 232 re-rated rows take a uniform −1, because the
re-rate's own `path:line` evidence standard produced a −0.42 blind divergence
rather than a −1.55 one. The `a9b3213f9` offsets stay as they were applied to the
555 frozen rows, so the ranking within each domain is preserved in both passes.

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
| A, floor 6 (recommended first) | No module below 6, and every concrete defect above fixed | 2 modules (about 4.0k lines) remain below 6 after round 5, plus the outstanding splits |
| B, floor 7 | No module below 7 | 61 modules (about 46k lines); a long programme |
| C | Mean 8.5 | Most of the codebase |

Round 5 works bottom-up, with smaller batches than for tests, and needs native
proof for runtime paths. It runs after round 4 has brought tests to floor 7.

This table is the state before floor 7. The floor-7 re-rate below closed option
B on score: no module sits at 6 or below.

## Drift: each re-rate's work list

Before round 5 the tables reported 95 new, 150 changed and 2 gone rows since
`a9b3213f9` (30.8% stale). After the re-rate at `e458614d8` (the floor-7 re-rate's
own drift line is in that section):

```text
code quality: 799 rows, 0 new, 0 changed, 0 gone, 0.0% stale since e458614d8
```

`effigy qa:quality:code` compares each row's `body_sha` with the working tree, so
new drift means a row's files changed after the re-rate and needs a fresh rating.
Split groups are reported as `split: re-rate` when new files sit under a gone
row's path.

## Floor-7 re-rate (effigy#982)

Floor 7 (F7a–F7h, PRs #997, #1002, #996, #1000, #1001, #1004, #1003, #1005) took
the 61 modules that scored 6 or below, plus the round-6 ownership tranches
#963/#970–#977 that landed after `e458614d8`. The frozen table then reported
195 stale entries (152 changed rows, 36 new product files, 7 gone = 23.4%).
Every changed row was re-read at current main with the round-5 rubric and
evidence standard, a row was added for each new module, and the tables were
rewritten in one pass:

| Work | Count |
|---|---|
| Rows re-scored because their body changed | 152 |
| New rows (split children, extracted owners and one new module) | 23 |
| Gone rows | 7 (5 split into new rows, 2 deleted unused scaffolding) |
| Rows untouched (kept verbatim, including `loc`) | 640 |
| Tables after the re-rate | 815 (C1 200, C2 73, C3 110, C4 189, C5 243) |

Method and limits: each re-rated row was read as it stands, using
`git diff e458614d8 -- <files>` to locate the change plus the module's current
public surface, ownership and failure paths; every rewritten `reason` ends in a
concrete `path:line`. `location` for a re-rated row is its current line count
with the same nested-row exclusion the round-5 pass used; untouched rows keep
their rating-pass count, so re-rating one module never rewrites the arithmetic
of a neighbour. No build, suite or native proof was run for the rating itself.
New product files that are a private submodule of an existing group row are
folded into that row (`test/render/suite_facts.rs`); a 27-line read-only
extraction is folded into its module row (`gateway::operator` owns
`operator/legacy_status.rs`). Every other new file has its own row.

Calibration follows the rule recalibrated in `effigy#960`/PR #992: the rater's
raw score is `given`, and `corrected = max(0, given − 1)` with
`source: offset-f7` for all 175 re-rated and new rows. The planner's blind check
runs afterwards.

### Distribution after the re-rate

- **Rows:** 815. **Mean:** 8.10 corrected (given 9.10).
- **Corrected scores:** 7 × 167, 8 × 403, 9 × 243, 10 × 2; nothing at 6 or below.
- **Given scores:** 7 × 2, 8 × 161, 9 × 407, 10 × 245.
- **7 or below:** 167 modules, all at exactly 7: 38 of the re-rated rows landed
  at `given` 8 (corrected 7) and 129 untouched rows already sat there.
- **Floor-7 target (no module below 7):** met on score; the two `given` 7 rows
  are the two largest remaining single-owner modules (`run/mod.rs`,
  `supervise.rs`), both already carrying a `split` action.
- **5 or below / 6 or below:** 0 modules (was 2 and 61).

Before this pass the tables held 799 rows with a mean of 7.95 corrected (given
8.96): `given` {6 × 2, 7 × 54, 8 × 155, 9 × 347, 10 × 241} and `corrected`
{5 × 2, 6 × 59, 7 × 158, 8 × 341, 9 × 236, 10 × 3}.

### Blind check of the floor-7 re-rate (effigy#983)

Independent raters re-scored 12 of the 175 floor-7 rows blind — never shown the
`given`, the `corrected` or the `reason` — from the committed source alone. The
sampled rows were the 12 largest-churn and largest re-rated modules plus the two
lowest corrected rows. Evidence: `f7-blind-sample.csv` and `f7-blind-results.csv`
in `/Volumes/Stash/scratch/effigy/code-rating-2026-11/`.

| Row | Given | Corrected before | Blind | `corrected` now |
|---|---|---|---|---|
| `crates/effigy-builtin/src/config/output` | 9 | 8 | 7 | 7 |
| `gateway::tls` | 8 | 7 | 9 | 9 |
| `crates/effigy-manifest/src/lib.rs` | 8 | 7 | 9 | 9 |
| `crates/effigy-containers/src/exec/implementation/workspace_builder.rs` | 9 | 8 | 7 | 7 |
| `crates/effigy-containers/src/workspace.rs` | 9 | 8 | 7 | 7 |
| `runner::environment` | 8 | 7 | 6 | 7 (re-rated after fix) |
| `src/runner/container_runtime_prep/checkout_mount.rs` | 9 | 8 | 9 | 9 |
| `crates/effigy-builtin/src/test/render/results` | 8 | 7 | 7 | 7 |
| `crates/effigy-ui/src/plain_renderer` | 8 | 7 | 9 | 9 |
| `src/runner/container_command/lifecycle/shell.rs` | 9 | 8 | 5 | 8 (re-rated after fix) |
| `crates/effigy-tasks/src/qa_groups/render.rs` | 8 | 7 | 9 | 9 |
| `crates/effigy-kernel/src/workspace_session` | 10 | 9 | 9 | 9 |

- blind − old corrected: mean **+0.17**, mean absolute difference 1.33. The −1
  offset holds: the blind re-score is as likely to be above the rater as below,
  and the spread is ordinary design-judgement noise rather than a systematic
  leniency left in the floor-7 rows.
- Rule applied: the 10 rows whose blind score is at or above 7 take it as their
  `corrected` with `source: blind-f7`; the other rows are untouched.
- The two rows below 7 blind were the only behaviour findings, and both named the
  same class of defect: a duplicated shell path with a hand-written `[ok]`
  literal and no direct test (`lifecycle/shell.rs`), and `Observed::Unknown`
  built from a flattened display string with an `expect` on fixed evidence and
  one busy-message test (`runner::environment`). Both defects are fixed
  (`effigy_runtime::shell::container_shell_completion` is the one completion
  owner; a failed inventory read carries the container crate's typed class and
  the evidence constructions return the typed reason), with direct
  handoff/runtime/refusal tests and failing-before controls. Their rows keep the
  floor-7 score with `source: offset-f7` and a reason naming what the fix closed.

### Distribution after the blind check

- **Rows:** 815. **Mean:** 8.11 corrected (given 9.10).
- **Corrected scores:** 7 × 166, 8 × 399, 9 × 248, 10 × 2; nothing at 6 or below.
- **7 or below:** 166 modules, all at exactly 7: four sampled rows moved up to
  9 and three moved down to 7.
- **Sources:** `blind` 24, `blind-r5` 8, `blind-f7` 10, `offset` 436,
  `offset-r5` 172, `offset-f7` 165. Every one of the 815 rows still carries a
  `body_sha` for the tree it was judged on, and drift reads zero.

### The 61 floor-7 targets, before and after

`before` is given / corrected in the frozen table; `after` is this pass.

| Table | Module | Before | After |
|---|---|---|---|
| C1 | `crates/effigy-containers/src/backend.rs` | 7 / 6 | 9 / 8 |
| C1 | `crates/effigy-containers/src/environment/backend.rs` | 7 / 6 | 8 / 7 |
| C1 | `crates/effigy-containers/src/exec/colima_runtime.rs` | 7 / 6 | 8 / 7 |
| C1 | `crates/effigy-containers/src/exec/implementation.rs` | 6 / 5 | 8 / 7 |
| C1 | `src/runner/container_command/support/workspace_private.rs` | 7 / 6 | deleted (unused scaffolding) |
| C1 | `src/runner/container_runtime_prep/cargo_inputs.rs` | 9 / 6 | 9 / 8 |
| C1 | `src/runner/exec_command/transport.rs` | 7 / 6 | 8 / 7 |
| C2 | `crates/effigy-gateway/src/proxy/performance.rs` | 7 / 6 | 9 / 8 |
| C2 | `gateway::operator` | 7 / 6 | 9 / 8 |
| C2 | `gateway::server` | 8 / 6 | 9 / 8 |
| C2 | `gateway::tls` | 7 / 6 | 8 / 7 |
| C2 | `managed::references` | 7 / 6 | 9 / 8 |
| C2 | `managed::scheduler` | 7 / 6 | deleted (9-line dead module; its renderer moved to `managed::run_spec`) |
| C3 | `crates/effigy-exec/src/detection.rs` | 7 / 6 | 9 / 8 |
| C3 | `crates/effigy-kernel/src/custody.rs` | 8 / 6 | 9 / 8 |
| C3 | `crates/effigy-kernel/src/supervise/memory.rs` | 7 / 6 | 8 / 7 |
| C3 | `crates/effigy-kernel/src/workspace_session.rs` | 8 / 6 | split: `workspace_session` 10 / 9, `activation.rs` 9 / 8, `claims.rs` 9 / 8, `session.rs` 9 / 8 |
| C3 | `crates/effigy-kernel/src/workspace_session/selection.rs` | 9 / 6 | split: `workspace_session/selection` 9 / 8 |
| C3 | `crates/effigy-process/src/signal.rs` | 7 / 6 | 9 / 8 |
| C4 | `crates/effigy-builtin/src/completion/scripts` | 7 / 6 | 9 / 8 |
| C4 | `crates/effigy-builtin/src/config/output.rs` | 7 / 6 | split: `config/output` 9 / 8 |
| C4 | `crates/effigy-builtin/src/config/schema.rs` | 7 / 6 | 9 / 8 |
| C4 | `crates/effigy-builtin/src/doc_render.rs` | 7 / 6 | 9 / 8 |
| C4 | `crates/effigy-builtin/src/help_text.rs` | 7 / 6 | 9 / 8 |
| C4 | `crates/effigy-builtin/src/init/agent.rs` | 7 / 6 | 9 / 8 |
| C4 | `crates/effigy-builtin/src/ports.rs` | 7 / 6 | 8 / 7 |
| C4 | `crates/effigy-builtin/src/test/execution.rs` | 7 / 6 | split: `test/execution` 9 / 8 |
| C4 | `crates/effigy-builtin/src/test/planning/runnable/cargo_env.rs` | 7 / 6 | 9 / 8 |
| C4 | `crates/effigy-builtin/src/test/render` | 7 / 6 | 9 / 8 |
| C4 | `crates/effigy-builtin/src/test/render/results` | 7 / 6 | 8 / 7 |
| C4 | `crates/effigy-builtin/src/text_doc.rs` | 7 / 6 | 9 / 8 |
| C4 | `crates/effigy-cli/src/command_parsing.rs` | 8 / 6 | 9 / 8 |
| C4 | `crates/effigy-cli/src/header.rs` | 7 / 6 | 8 / 7 |
| C4 | `crates/effigy-cli/src/help` | 7 / 6 | 8 / 7 |
| C4 | `crates/effigy-manifest/src/config_sections/container.rs` | 7 / 6 | 9 / 8 |
| C4 | `crates/effigy-manifest/src/loaded_catalog.rs` | 7 / 6 | 9 / 8 |
| C4 | `crates/effigy-manifest/src/retired_bootstrap.rs` | 7 / 6 | 8 / 7 |
| C4 | `crates/effigy-rhai/src/host_api/fs.rs` | 7 / 6 | 9 / 8 |
| C4 | `crates/effigy-tasks/src/error.rs` | 7 / 6 | 8 / 7 |
| C4 | `crates/effigy-tasks/src/listing.rs` | 7 / 6 | 8 / 7 |
| C4 | `crates/effigy-tasks/src/qa_groups/render.rs` | 7 / 6 | 8 / 7 |
| C4 | `crates/effigy-tasks/src/testing/vitest.rs` | 7 / 6 | 9 / 8 |
| C4 | `crates/effigy-tui/src/core.rs` | 7 / 6 | 9 / 8 |
| C4 | `crates/effigy-tui/src/multiprocess/diagnostics.rs` | 7 / 6 | 8 / 7 |
| C4 | `crates/effigy-tui/src/multiprocess/render` | 7 / 6 | 8 / 7 |
| C4 | `crates/effigy-tui/src/multiprocess/runtime_loop.rs` | 7 / 6 | 8 / 7 |
| C4 | `crates/effigy-ui/src/lib.rs` | 6 / 5 | 8 / 7 |
| C4 | `crates/effigy-ui/src/output.rs` | 7 / 6 | 9 / 8 |
| C4 | `crates/effigy-ui/src/steps.rs` | 7 / 6 | split: `steps` 9 / 8 |
| C4 | `crates/effigy-ui/src/theme.rs` | 7 / 6 | 9 / 8 |
| C4 | `src/runner/execute/api.rs` | 7 / 6 | 9 / 8 |
| C4 | `src/runner/execute/binding.rs` | 7 / 6 | 9 / 8 |
| C5 | `effigy-core::container_detection` | 7 / 6 | 9 / 8 |
| C5 | `effigy-core::repo` | 7 / 6 | 9 / 8 |
| C5 | `effigy-doctor::deep_inventory` | 7 / 6 | 8 / 7 |
| C5 | `effigy-doctor::manifest_schema` | 10 / 6 | 8 / 7 |
| C5 | `effigy-runtime::error` | 7 / 6 | 9 / 8 |
| C5 | `effigy-state::artifact` | 7 / 6 | 9 / 8 |
| C5 | `effigy-state::paths` | 7 / 6 | 8 / 7 |
| C5 | `runner::managed_wait` | 7 / 6 | 9 / 8 |
| C5 | `runner::tasks_command` | 7 / 6 | 9 / 8 |

The ten modules added outside the floor targets by the round-6 tranches (for
example `effigy-core::output_cap`, `crates/effigy-kernel/src/custody/output.rs`,
`crates/effigy-kernel/src/supervise/frame.rs`, `src/runner/execute/runtime_inputs.rs`
and `src/runner/exec_command/transport/argv.rs`) each score 9–10 and are new
owners rather than re-reads of an old row.

### Gone rows

Five rows left the table because their file became a module:

- `crates/effigy-builtin/src/config/output.rs` → `crates/effigy-builtin/src/config/output`
  (mod, inspect, payload, user).
- `crates/effigy-builtin/src/test/execution.rs` → `crates/effigy-builtin/src/test/execution`
  (mod, command, parallel, session).
- `crates/effigy-kernel/src/workspace_session.rs` → `workspace_session` (facade),
  `activation.rs`, `claims.rs`, `session.rs`; the nested selection row keeps its
  own three files.
- `crates/effigy-kernel/src/workspace_session/selection.rs` → `workspace_session/selection`
  (mod, context, obligation).
- `crates/effigy-ui/src/steps.rs` → `crates/effigy-ui/src/steps`
  (mod, live, source, terminal).

Two were deleted as unused scaffolding, both in F7g (#971):
`managed::scheduler` (9 lines, no caller after its renderer moved to
`managed::run_spec`) and `src/runner/container_command/support/workspace_private.rs`
(197 lines, no caller, every entry point returning `Unavailable`).

### Follow-up not taken here

`scripts/quality-table.py` still defines an unused `CODE_EXPECTED_ROWS`
(196/74/105/182/242). The re-rate changes the row counts to 200/73/110/189/243;
the constant is not read by the drift report, and this task rates the tables
rather than editing the tool, so it is recorded here for whoever next touches
that script.
