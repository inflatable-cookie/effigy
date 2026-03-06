# 017 - Comment Ratio Scan And Doctor Integration

Generation: `g01`

Status: Planned
Owner: Platform
Created: 2026-03-06
Depends on: 009, 012, 014, 016

## Vision Alignment

This roadmap expands Effigy's scan and doctor system so files dominated by comment/docs volume can be surfaced before teams waste time treating documentation-heavy files as oversized code or miss files whose implementation signal has been buried under deferred commentary.

## Primary Tags

- `OPERATE`
- `CONTRACT`
- `MAINT`

## Target Envelope

- Effigy can detect source files where comment/docs volume materially outweighs executable code and surface them consistently through both `scan` and `doctor`.

## Vision Target Delta

- Move from size-only and duplication-only structural health scanning toward a ratio-based quality signal that distinguishes true implementation bulk from commentary-heavy files.

## 1) Problem

Effigy can already find oversized code files, duplicate blocks, generated assets, and explicit attention markers, but it still lacks a way to answer a common maintenance question: is this file actually large in executable logic, or is it mostly comments/docs?

Without this distinction teams still waste time on:
- chasing “god files” that are mostly commentary or generated-style notes,
- overlooking files where docs/comments have grown faster than the implementation itself,
- misclassifying AI-generated files padded with long explanatory comment blocks,
- treating commentary-heavy files as the same problem as duplication-heavy or logic-heavy files.

The result is noisy triage and poor separation between “too much code” and “too much commentary around a small amount of code”.

## 2) Goals

- [x] Add `effigy scan comment-ratio` as a first-class built-in scanner.
- [x] Detect source/test files where comment/docs-only lines materially outweigh executable lines.
- [x] Reuse the current scan traversal model, including child-catalog fanout and `.gitignore` handling.
- [x] Support text, markdown, and JSON output with stable schema versioning.
- [x] Integrate comment-ratio findings into `effigy doctor`.
- [x] Make thresholds, minimum file size, and traversal rules configurable via `effigy.toml`.

## 3) Non-Goals

- [ ] No semantic “comment quality” scoring in v1.
- [ ] No natural-language classification of whether comments are good or bad.
- [ ] No docs-site or markdown scanning by default.
- [ ] No line-by-line suggestion engine for comment removal.
- [ ] No AST-level doc-comment distinction beyond heuristic language support.

## 4) Scanner Contract

Command surface:
- `effigy scan comment-ratio`
- `effigy scan comment-ratio --show-warnings`
- `effigy scan comment-ratio --markdown --out reports/comment-ratio.md`
- `effigy --json scan comment-ratio`

Core finding model:
- `path`
- `code_lines`
- `comment_lines`
- `ratio`
- `severity`

Top-level metrics:
- `scanned-files`
- `candidate-files`
- `findings`

Default severity direction:
- `warning`: comment/code ratio at or above `1.5`
- `high`: comment/code ratio at or above `2.0`
- `critical`: comment/code ratio at or above `3.0`

Default minimum-size direction:
- only evaluate files with at least `20` code lines so tiny files do not create noise

Default scope:
- include source and test files
- exclude docs, generated artefacts, migrations, fixtures, examples, benchmarks, lockfiles
- respect `.gitignore`
- fan out across child catalogs/sub-repos like the existing scanners

Default normalization direction:
- count comment-only lines rather than mixed code/comment lines
- count doc-comment lines in supported languages
- ignore blank lines
- fall back conservatively for unknown extensions

## 5) Manifest Contract

Add scanner configuration under:

```toml
[scan.comment_ratio]
doctor = true
respect_gitignore = true
fail_on_findings = false
include = ["src/**", "crates/**", "tests/**"]
exclude = ["vendor/**"]
warn = 1.5
high = 2.0
critical = 3.0
min_code_lines = 20
format = "text"
out = "reports/comment-ratio.md"
```

The command should allow CLI overrides for:
- output mode (`--json`, `--markdown`, `--out`)
- traversal (`--no-gitignore`, `--include`, `--exclude`)
- reporting verbosity (`--show-warnings`)
- fail mode (`--fail-on-findings`)
- ratio thresholds (`--warn`, `--high`, `--critical`)
- minimum file size (`--min-code-lines`)

Initial doctor policy:
- ship doctor integration in the roadmap
- after `acowtancy` benchmark validation, keep `[scan.comment_ratio].doctor = true` as the default

## 6) Detection Strategy

Use a deterministic, bounded ratio-based approach in v1:

1. normalize supported source files into executable vs comment/docs-only line classes
2. count code-only lines and comment/docs-only lines per file
3. discard files below `min_code_lines`
4. compute `ratio = comment_lines / code_lines`
5. classify findings by ratio thresholds
6. sort by severity, ratio, then comment volume

Expected properties:
- deterministic output
- stable enough for JSON contracts and CI reports
- useful as a companion to `god-files`, not a replacement for it
- performant enough for medium repos, with default doctor participation after validation

## 7) Execution Plan

### Batch 17.1 - Scanner Core
- [x] Add the `comment-ratio` scan command and argument parsing.
- [x] Implement code/comment/docs-only line classification across the existing scan traversal pipeline.
- [x] Count `scanned-files`, `candidate-files`, and `findings`.
- [x] Reuse child-catalog workspace fanout and bounded ignore handling.

### Batch 17.2 - Output and Contracts
- [x] Add text rendering with warning-row suppression and `--show-warnings`.
- [x] Add markdown rendering and file output support.
- [x] Add schema-versioned JSON payloads and CLI envelope coverage.
- [x] Add manifest config support and schema docs for `[scan.comment_ratio]`.

### Batch 17.3 - Doctor Integration
- [x] Add `scan.comment-ratio` as a doctor-backed check.
- [x] Support doctor participation through `[scan.comment_ratio].doctor = true`.
- [x] Summarize comment-ratio findings in doctor text and write file-level details to `.effigy/reports/doctor/scan-comment-ratio.md`.

### Batch 17.4 - Documentation and Validation
- [x] Update command docs, manifest cookbook, JSON examples, and snippets.
- [x] Add regression coverage for nested repos, `.gitignore` behavior, and language fallback behavior.
- [x] Benchmark real repos such as `acowtancy` to validate runtime and noise before enabling doctor by default.

## 8) Acceptance Criteria

- [x] `effigy scan comment-ratio` finds commentary-heavy files across nested catalogs.
- [x] Default output is concise in terminal text and complete in markdown/JSON.
- [x] `effigy doctor` can include comment-ratio findings when enabled.
- [x] Manifest config can tune ratio thresholds, minimum file size, and output defaults.
- [x] Help/docs/contracts clearly describe comment counting rules, exclusions, and doctor-default behavior.

## 9) Risks and Mitigations

- [ ] Risk: comment classification differs too much between languages.
  - Mitigation: support a clear common language set first and use conservative fallbacks elsewhere.
- [ ] Risk: files with useful narrative docs create too much noise.
  - Mitigation: require `min_code_lines`, rank by ratio plus comment volume, and allow doctor opt-out where needed.
- [ ] Risk: mixed code/comment lines blur the ratio signal.
  - Mitigation: count comment-only lines in v1 and document that boundary explicitly.
- [ ] Risk: this overlaps confusingly with `god-files`.
  - Mitigation: document it as a companion scan that explains commentary-heavy outliers rather than replacing size-based scans.

## 10) Deliverables

- [x] `effigy scan comment-ratio`
- [x] `[scan.comment_ratio]` manifest contract
- [x] `doctor` integration for `scan.comment-ratio`
- [x] text/markdown/JSON contract coverage
- [x] updated command/config/docs coverage

## 11) Validation

- [x] `cargo test run_manifest_task_builtin_scan_ --lib`
- [x] `cargo test scan_contract_tests --lib`
- [x] `cargo test doctor_json_contract_ --lib`
- [x] `cargo test cli_json_mode_scan_ --test cli_output_tests`
- [x] `bash docs/scripts/check-vision-metadata.sh`
- [x] benchmark logs for at least one real repo before changing default doctor participation

Benchmark summary:
- `acowtancy` standalone `scan comment-ratio` run: `scanned-files=1905`, `candidate-files=1472`, `findings=15`, `real=2.41s`
- Decision: keep `[scan.comment_ratio].doctor = true` as the default because runtime and noise are acceptable for default health runs.

## 12) Next Task

Roadmap `g01.017` is complete. Reassess the next scan milestone and decide whether the next target should stay hygiene-focused (`generated-in-src`, `mixed-responsibility`) or move toward structural policy checks (`layering`, `dead-exports`).
