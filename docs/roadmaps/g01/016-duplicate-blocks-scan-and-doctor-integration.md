# 016 - Duplicate Blocks Scan and Doctor Integration

Generation: `g01`

Status: Complete
Owner: Platform
Created: 2026-03-06
Depends on: 009, 012, 014

## Vision Alignment

This roadmap expands Effigy's scan and doctor system so structurally duplicated code blocks can be surfaced before AI-generated drift hardens into parallel implementations that are expensive to unwind.

## Primary Tags

- `OPERATE`
- `CONTRACT`
- `MAINT`

## Target Envelope

- Effigy can detect suspiciously large duplicated code blocks across source trees and surface them consistently through both `scan` and `doctor`.

## Vision Target Delta

- Moved from file-size and marker-based health scanning toward structural duplication detection for repo hygiene and AI-generated code cleanup.

## 1) Problem

Effigy can now identify oversized code files, bulky generated assets, and explicit attention markers, but it still misses one of the most common cleanup burdens in AI-assisted repos: large duplicated code blocks spread across multiple files.

In practice this means teams still spend time manually hunting down:
- copied handlers or service implementations that evolved in parallel,
- duplicated UI or API logic created by adjacent prompts,
- repeated integration-test setup blocks that should be shared,
- cloned helper modules that differ only by naming or minor formatting.

Without a dedicated duplicate-block scanner, this information remains invisible to `doctor`, CI, and report workflows until the cleanup cost is already high.

## 2) Goals

- [x] Add `effigy scan duplicate-blocks` as a first-class built-in scanner.
- [x] Detect repeated normalized code blocks across source files and test files.
- [x] Reuse the current scan traversal model, including child-catalog fanout and `.gitignore` handling.
- [x] Support text, markdown, and JSON output with stable schema versioning.
- [x] Integrate duplicate-block findings into `effigy doctor`.
- [x] Make duplicate thresholds and traversal rules configurable via `effigy.toml`.

## 3) Non-Goals

- [ ] No AST-based clone detection in v1.
- [ ] No fuzzy semantic or embedding-based duplicate detection.
- [ ] No intra-file duplicate reporting in v1.
- [ ] No docs/content duplication scanning by default.
- [ ] No framework-specific boilerplate suppression beyond simple heuristic filters.

## 4) Scanner Contract

Command surface:
- `effigy scan duplicate-blocks`
- `effigy scan duplicate-blocks --show-warnings`
- `effigy scan duplicate-blocks --markdown --out reports/duplicate-blocks.md`
- `effigy --json scan duplicate-blocks`

Core finding model:
- `severity`
- `block_lines`
- `occurrences`
- `fingerprint`
- `snippet`
- `locations`

Per-location model:
- `path`
- `start_line`
- `end_line`

Top-level metrics:
- `scanned-files`
- `candidate-blocks`
- `findings`

Default severity direction:
- `warning`: duplicated normalized block at or above `20` lines across `2` locations
- `high`: duplicated normalized block at or above `40` lines or across `3+` locations
- `critical`: duplicated normalized block at or above `80` lines or across `4+` locations

Default scope:
- include source and test files
- exclude docs, generated artefacts, migrations, fixtures, examples, benchmarks, lockfiles
- respect `.gitignore`
- fan out across child catalogs/sub-repos like `god-files`

Default normalization direction:
- trim whitespace noise
- collapse blank-line-only variation
- ignore tiny blocks
- suppress obvious import/header-only windows
- optionally strip comment-only lines for supported languages where that is safe

## 5) Manifest Contract

Add scanner configuration under:

```toml
[scan.duplicate_blocks]
doctor = false
respect_gitignore = true
fail_on_findings = false
include = ["src/**", "crates/**", "tests/**"]
exclude = ["vendor/**"]
warn = 20
high = 40
critical = 80
min_occurrences = 2
format = "text"
out = "reports/duplicate-blocks.md"
```

The command should allow CLI overrides for:
- output mode (`--json`, `--markdown`, `--out`)
- traversal (`--no-gitignore`, `--include`, `--exclude`)
- reporting verbosity (`--show-warnings`)
- fail mode (`--fail-on-findings`)
- duplication thresholds (`--warn`, `--high`, `--critical`)

Initial doctor policy:
- ship doctor integration in the roadmap
- default `[scan.duplicate_blocks].doctor = false` until runtime/noise is validated on real repos

## 6) Detection Strategy

Use a deterministic, bounded structural approach in v1:

1. normalize candidate source lines per file
2. generate fixed-size sliding windows over normalized lines
3. hash windows to collect repeated windows across files
4. merge overlapping repeated windows into maximal duplicate blocks
5. rank duplicate groups by block size and occurrence count
6. suppress obviously low-value boilerplate matches

Expected properties:
- deterministic output
- stable enough for JSON contracts and CI reports
- performant enough for medium repos, with explicit doctor opt-in at first

## 7) Execution Plan

### Batch 16.1 - Scanner Core
- [x] Add the `duplicate-blocks` scan command and argument parsing.
- [x] Implement normalized duplicate-window detection across the existing scan traversal pipeline.
- [x] Count `scanned-files`, `candidate-blocks`, and `findings`.
- [x] Reuse child-catalog workspace fanout and bounded ignore handling.

### Batch 16.2 - Output and Contracts
- [x] Add text rendering with warning-row suppression and `--show-warnings`.
- [x] Add markdown rendering and file output support.
- [x] Add schema-versioned JSON payloads and CLI envelope coverage.
- [x] Add manifest config support and schema docs for `[scan.duplicate_blocks]`.

### Batch 16.3 - Doctor Integration
- [x] Add `scan.duplicate-blocks` as a doctor-backed check.
- [x] Support doctor participation through `[scan.duplicate_blocks].doctor = true`.
- [x] Summarize duplicate-block findings in doctor text and write file-level details to `.effigy/reports/doctor/scan-duplicate-blocks.md`.

### Batch 16.4 - Documentation and Validation
- [x] Update command docs, manifest cookbook, JSON examples, and snippets.
- [x] Add regression coverage for nested repos, `.gitignore` behavior, and default exclusions.
- [x] Benchmark real repos such as `example-app` to validate runtime and noise before enabling doctor by default.

## 8) Acceptance Criteria

- [x] `effigy scan duplicate-blocks` finds repeated normalized code blocks across nested catalogs.
- [x] Default output is concise in terminal text and complete in markdown/JSON.
- [x] `effigy doctor` can include duplicate-block findings when enabled.
- [x] Manifest config can tune thresholds, occurrence policy, and output defaults.
- [x] Help/docs/contracts clearly describe exclusions, normalization, and doctor opt-in behavior.

## 9) Risks and Mitigations

- [ ] Risk: scanner runtime is too slow for default `doctor`.
  - Mitigation: ship doctor integration but keep it opt-in until measured on real repos.
- [ ] Risk: false positives from shared boilerplate or generated patterns.
  - Mitigation: require meaningful block sizes, suppress import/header-only windows, and exclude generated/default-noise paths.
- [ ] Risk: duplicate windows explode memory usage on large repos.
  - Mitigation: bound window sizes, merge aggressively, and rank/suppress low-value candidates early.
- [ ] Risk: output is too noisy to action.
  - Mitigation: keep terminal text summary-first, hide warning rows by default, and move detail rows into reports.

## 10) Deliverables

- [x] `effigy scan duplicate-blocks`
- [x] `[scan.duplicate_blocks]` manifest contract
- [x] `doctor` integration for `scan.duplicate-blocks`
- [x] text/markdown/JSON contract coverage
- [x] updated command/config/docs coverage

## 11) Validation

- [x] `cargo test run_manifest_task_builtin_scan_ --lib`
- [x] `cargo test scan_contract_tests --lib`
- [x] `cargo test doctor_json_contract_ --lib`
- [x] `cargo test cli_json_mode_scan_ --test cli_output_tests`
- [x] `bash docs/scripts/check-vision-metadata.sh`
- [x] benchmark logs for at least one real repo before changing default doctor participation

Benchmark note:
- `example-app` standalone `scan duplicate-blocks` run: `scanned-files=1905`, `candidate-blocks=207604`, `findings=95`, `real=16.85s`
- Decision: keep `[scan.duplicate_blocks].doctor = false` as the default because the signal is useful but still too expensive/noisy for default doctor runs.

## 12) Next Task

Roadmap `g01.016` is complete. Reassess scanner backlog priorities and open the next scan milestone only after deciding whether the next target should be cleanup-oriented (`mixed-responsibility`, `comment-ratio`) or policy-oriented (`generated-in-src`, `layering`).
