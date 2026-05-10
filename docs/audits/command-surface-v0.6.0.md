# Effigy Command Surface Audit — v0.6.0 Recommendations

Generated: 2026-05-10
Scope: User-facing CLI commands, subcommands, flags, and naming conventions

## Executive Summary

Effigy recently underwent a significant command surface consolidation (post-0.5.0,
in `[Unreleased]`): `migrate`/`unlock`/`cache` moved under `tasks`, `completion`
moved under `config`, and the `catalogs` alias was removed. This audit focuses on
what remains to tidy up for a clean v0.6.0 surface.

**Status:** The major reorganization is complete. Remaining work is polish,
consistency, and removal of duplication.

---

## 1. User-Facing Issues (Recommended for v0.6.0)

### 1.1 Remove `release gates` — Duplicate Entry Point

**Current:** `release gates` and `release status --check-gates` do the same thing.

**Recommendation:** Remove `release gates`. Keep only `release status --check-gates`.

**Rationale:** One command should not have two ways to express the same
operation. `status --check-gates` is more explicit about what it does.

**Breaking:** Yes. Anyone using `effigy release gates` must switch to
`effigy release status --check-gates`.

---

### 1.2 Normalize `bootstrap` Compound Subcommands

**Current:** `bootstrap deps sync`, `bootstrap children status`, `bootstrap
children sync` are the only commands using space-separated multi-word
subcommands without dashes.

**Recommendation:** Change to dashed forms:
- `bootstrap deps-sync`
- `bootstrap children-status`
- `bootstrap children-sync`

**Rationale:** Every other subcommand in Effigy uses single words or dashes
(`check-links`, `validate-metadata`, `reset-runtime`). Bootstrap is the lone
exception.

**Breaking:** Yes. All three command spellings change.

---

### 1.3 Rename `demo browser` to `demo browse` or `demo list --interactive`

**Current:** `demo browser` is a noun where every other `demo` subcommand is a
verb (`run`, `stop`, `input`, `resize`, `rerun`, `list`, `inspect`, `history`).

**Recommendation:** Rename to `demo browse` (verb) or merge into `demo list
--interactive`.

**Rationale:** Consistent verb-based CLI grammar. `browse` implies the same
operation as `browser` but uses a verb.

**Breaking:** Yes. `demo browser` becomes `demo browse`.

---

### 1.4 Consolidate `docs check-*` Subcommands

**Current:** 10 subcommands: `check-links`, `check-json-examples`,
`check-headings`, `check-paths`, `check-contains`, `check-forbidden`,
`check-index`, `check-next-action`, `check-workflow-paths`, `add-log-index`.

**Recommendation:** Collapse into `docs check <KIND>` with a required positional
kind argument, plus `docs add-log-index` as the odd one out.

```
effigy docs check links [PATHS...]
effigy docs check headings [PATHS...] --require-heading
...
```

**Rationale:** The check subcommands all share the same signature and dispatch
to the same `checks.rs` module. The current surface is unnecessarily broad.

**Breaking:** Yes. All `docs check-*` commands change to `docs check *`.

---

### 1.5 Decide on `artifact` vs `artefact` Spelling Alias

**Current:** Both `artifact` and `artefact` are accepted at the CLI parser level.

**Recommendation:** Pick one and remove the alias. Prefer `artifact` (American
spelling, used in code and docs).

**Rationale:** No other command has a spelling alias. This is unnecessary
surface complexity.

**Breaking:** Yes. `artefact` stops working.

---

### 1.6 Document `version` Command in Reference Guide

**Current:** `effigy version` and `effigy --version` exist in the CLI parser but
are missing from `docs/guides/025-command-reference-matrix.md`.

**Recommendation:** Add `version` to the Primary Commands table and Command
Shapes section.

**Breaking:** No.

---

### 1.7 Fix Missing `container` Subcommand Shapes in Guide

**Current:** `container cache prune` and `container volume prune` are missing
from the Command Shapes section. Several flags (`--project`, `--kind` on cache
commands; `--push` on data dump) are also undocumented.

**Recommendation:** Update `docs/guides/025-command-reference-matrix.md` to
include all subcommands and flags.

**Breaking:** No.

---

### 1.8 Consider `system` → `container` Alias or Deprecation

**Current:** `system up/down/status/logs` are 90% passthroughs to `container`
with a workspace context wrapper. `system repair` and `system reset-runtime` are
the only unique operations.

**Recommendation:** Either:
- Deprecate `system` and document it as an alias for `container` in workspace
  context, or
- Merge `system repair` and `system reset-runtime` into `container repair` and
  `container reset-runtime`, then remove `system` entirely.

**Rationale:** Two commands doing the same thing creates confusion about which
to use. `system` was originally a higher-level abstraction but has collapsed
into a container wrapper.

**Breaking:** Yes, if removed. Lower impact if kept as alias.

---

### 1.9 `--repo` Flag Availability

**Current:** Most commands support `--repo`. `changelog`, `bootstrap`, and
`bundle` do not.

**Recommendation:** Add `--repo` support to `changelog` (operates on repo files)
and `bundle` (operates on repo-local exports). `bootstrap` intentionally targets
a new directory, so `--repo` may not make sense there.

**Breaking:** No (additive).

---

## 2. Internal/Code Quality Issues (Recommended for v0.6.0 or v0.7.0)

### 2.1 Extract Common JSON/Text Dispatcher

Almost every command repeats:
```rust
if output_json {
    Ok(json!({...}).to_string())
} else {
    Ok(text_lines.join("\n"))
}
```

**Recommendation:** A shared `render_command_result(json_value, text_output)`
helper in `effigy_ui` or `runner::common`.

**Impact:** Removes ~200-400 lines of duplicated branching across
`artifact_command`, `release_command`, `gateway_command`, `state_command`, etc.

---

### 2.2 Split `container_command` (6549 lines)

The largest command module. `run_container` is a massive match block.

**Recommendation:** Split into:
- `container_command/lifecycle.rs` — up, down, status, stats, logs, shell, reset
- `container_command/data.rs` — data list, export, dump, import, seed, pull-production
- `container_command/cache.rs` — cache list, prune
- `container_command/volume.rs` — volume list, prune
- `container_command/eject.rs` — eject

**Impact:** No user-facing change. Improves compile times and maintainability.

---

### 2.3 Collapse `exec_command` Duplicated Variants

Four near-identical functions:
- `run_routed_task_container_exec`
- `capture_routed_task_container_exec`
- `run_routed_task_container_exec_with_policy`
- `capture_routed_task_container_exec_with_policy`

**Recommendation:** Collapse into two functions with a `capture: bool` and
`policy: Option<Policy>` parameter.

---

### 2.4 Replace `script_command` Manual Feature Match

`script_command` manually reconstructs CLI structs for 60+ Rhai features. This
is a maintenance hazard.

**Recommendation:** Generate the feature dispatch table from CLI definitions, or
use generic deserialization (`serde_json::from_value`) to build CLI structs from
Rhai feature descriptors.

---

### 2.5 Extract Release `plan/yes/interactive` Dispatcher

`release_command`'s `Prepare` and `Execute` subcommands repeat the same
`--plan` / `--yes` / interactive branching logic.

**Recommendation:** A single `run_release_stage(stage, plan, yes, allow_stale)`
helper.

---

### 2.6 Move `state_command` Report Structs to Submodule

`state_command.rs` is 1866 lines with report structs, text renderers, manifest
resolution, and history scanning all inline.

**Recommendation:** Move reports and renderers to `state_command/reports.rs`.

---

## 3. Domain Organization Observations

### 3.1 Release + Distribution Boundary

`release` and `distribution` are tightly coupled but separate. This is
intentional (release = orchestration, distribution = mechanics) and should
remain. However, the user-facing boundary could be clearer:

- `release` is what operators run during a release workflow.
- `distribution` is what CI/automation runs to validate and package artifacts.

**Recommendation:** Keep separate. Add a guide explaining when to use each.

---

### 3.2 State + Artifact Boundary

`state` (plan/apply/capture/history) and `artifact` (inspect/stage/capture) both
touch the OCI artifact substrate. `state capture` can publish to OCI; `artifact`
can stage local files to OCI.

**Current separation:** `state` is about manifest-defined infrastructure
workflows; `artifact` is about ad-hoc artifact operations.

**Recommendation:** Keep separate. The distinction is meaningful.

---

### 3.3 Container as the "God Command"

`container` has 11 direct subcommands, some with 2nd/3rd level nesting. It is by
far the largest surface area.

**Observation:** This is justified — containers are Effigy's core runtime
abstraction. But the depth (3 levels: `container data dump --db-dump`) is
unusual.

**Recommendation:** Accept the depth, but ensure subcommands are grouped
logically. `container data` and `container volume` are already well-grouped.
`container cache` could arguably merge with `container volume` (both are storage
management), but the separation is clean enough.

---

## 4. Implementation Priority for v0.6.0

### P1 — Breaking Changes (do together)
1. Remove `release gates`
2. Change `bootstrap deps sync` → `bootstrap deps-sync`, etc.
3. Rename `demo browser` → `demo browse`
4. Collapse `docs check-*` → `docs check <KIND>`
5. Remove `artefact` alias

### P2 — User-Facing Additions
6. Add `version` to reference guide
7. Fix missing `container` subcommand shapes and flags
8. Add `--repo` to `changelog` and `bundle`

### P3 — Structural Cleanup (no user impact)
9. Split `container_command` into submodules
10. Extract common JSON/text dispatcher
11. Collapse `exec_command` duplicated variants

### P4 — Deferred to v0.7.0
12. `system` deprecation/merge decision
13. `script_command` generation
14. `state_command` report extraction
15. Release stage dispatcher extraction

---

## 5. Changelog Impact

All P1 items are **Breaking** and belong under `[Unreleased] Breaking`.

All P2 items are **Changed** (docs fixes) or **Added** (`--repo` support).

P3/P4 items are internal refactoring with no changelog entry needed unless they
fix bugs.

---

## Appendix: Command Inventory Summary

| Command | Subcommands | Lines | Complexity | Notes |
|---------|------------|-------|------------|-------|
| `artifact` | inspect, stage, capture | 854 | Medium | |
| `bootstrap` | clone, teardown, deps-sync, children-status, children-sync | 3122 | Very High | Space-separated subcommands |
| `bundle` | list, inspect, export | 192 | Low | |
| `changelog` | validate, format, analyze, extract | 200 | Low | |
| `container` | up, down, status, stats, logs, shell, reset, data, cache, volume, eject | 6549 | Extremely High | Deepest tree |
| `contracts` | validate-selection, check-json | 112 | Low | |
| `demo` | list, browser, inspect, history, run, stop, input, resize, rerun | 2266 | Very High | `browser` is noun |
| `deploy` | model, export | 1351 | Medium-High | |
| `distribution` | validate-metadata, check-glibc-floor, preflight, first-publish, validate-artifacts, generate-closeout, write-summary | 545 | Medium | |
| `docs` | check-links, check-json-examples, check-headings, check-paths, check-contains, check-forbidden, check-index, check-next-action, check-workflow-paths, add-log-index | 723 | Medium | 10 check subcommands |
| `exec` | (single) | 2417 | High | 4 near-duplicate variants |
| `gateway` | up, down, status, setup-tls | 1631 | High | |
| `release` | status, gates, resume, simulate, prepare, execute, verify-install | 1867 | High | `gates` duplicates `status --check-gates` |
| `service` | list, extract | 154 | Low | |
| `state` | plan, apply, capture, history | 1866 | Very High | |
| `system` | up, down, status, logs, repair, reset-runtime | 2429 | High | 90% container passthrough |

**Total user-facing command surface:** 31 top-level commands, ~60+ subcommands.
