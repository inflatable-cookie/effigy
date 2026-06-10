# 014 - Attention Marker Scan and Doctor Integration

Generation: `g01`

Status: Complete
Owner: Platform
Created: 2026-03-06
Depends on: 009, 012, 013

## Vision Alignment

This roadmap expands Effigy's scan and doctor system so deferred work, deprecation markers, and explicit attention comments are visible before they turn into structural drift.

## Primary Tags

- `OPERATE`
- `CONTRACT`
- `MAINT`

## Target Envelope

- Effigy can detect explicit code attention markers across source trees and surface them consistently through both `scan` and `doctor`.

## Vision Target Delta

- Moved from structural health scanning only toward a broader repo-health model that also captures deferred work and code-level attention signals.

## 1) Problem

Effigy can now identify oversized code files and bulky generated assets, but it cannot yet surface the other common class of maintenance debt: explicit attention markers embedded in code. In practice this means teams still miss:
- `TODO` and `FIXME` trails left in active code paths,
- deprecation notices that need follow-up work,
- temporary workarounds and placeholders that should not silently accumulate,
- review notes and deferred cleanup markers that are only visible during manual inspection.

Without a dedicated scanner, this information remains scattered across repos and does not participate in `doctor`, CI, or report generation.

## 2) Goals

- [x] Add `effigy scan attention-markers` as a first-class built-in scanner.
- [x] Detect explicit marker categories for deferred work, deprecations, and developer-attention comments.
- [x] Reuse the current scan traversal model, including child-catalog fanout and `.gitignore` handling.
- [x] Support text, markdown, and JSON output with stable schema versioning.
- [x] Integrate attention-marker findings into `effigy doctor`.
- [x] Make marker sets and severity bands configurable via `effigy.toml`.

## 3) Non-Goals

- [ ] No fuzzy NLP-style inference from arbitrary prose comments.
- [ ] No AST parsing requirement in v1.
- [ ] No ticket ownership, age tracking, or auto-remediation workflow in this roadmap.
- [ ] No default scanning of docs/content trees.

## 4) Scanner Contract

Command surface:
- `effigy scan attention-markers`
- `effigy scan attention-markers --show-warnings`
- `effigy scan attention-markers --markdown --out reports/attention-markers.md`
- `effigy --json scan attention-markers`

Core finding model:
- `path`
- `line`
- `category`
- `severity`
- `marker`
- `snippet`

Top-level metrics:
- `scanned-files`
- `matched-lines`
- `findings`

Default category families:
- deferred work: `TODO`, `@TODO`, `FIXME`, `XXX`, `HACK`, `BUG`, `REVIEW`, `NOTE`
- deprecation: `deprecated`, `@deprecated`, `DEPRECATED`, common attribute/comment forms
- temporary/deferred artefacts: `follow up`, `temporary`, `remove before`, `workaround`, `tech debt`, `later`, `stub`, `placeholder`

Default severity direction:
- `critical`: security-sensitive or release-blocking deferred markers such as `BUG`, `SECURITY`, `remove before release`
- `high`: `FIXME`, `HACK`, strong deprecation markers, workarounds
- `warning`: `TODO`, `REVIEW`, `NOTE`, placeholders

Default scope:
- include source and test files
- exclude docs, generated artefacts, migrations, fixtures, examples, benchmarks, lockfiles
- respect `.gitignore`
- fan out across child catalogs/sub-repos like `god-files`

## 5) Manifest Contract

Add scanner configuration under:

```toml
[scan.attention_markers]
doctor = true
respect_gitignore = true
fail_on_findings = false
include = ["src/**", "crates/**", "tests/**"]
exclude = ["vendor/**"]
warning = ["TODO", "REVIEW", "NOTE", "placeholder"]
high = ["FIXME", "HACK", "@deprecated", "workaround"]
critical = ["BUG", "SECURITY", "remove before release"]
format = "text"
out = "reports/attention-markers.md"
```

The command should allow CLI overrides for:
- output mode (`--json`, `--markdown`, `--out`)
- traversal (`--no-gitignore`, `--include`, `--exclude`)
- reporting verbosity (`--show-warnings`)
- fail mode (`--fail-on-findings`)

## 6) Execution Plan

### Batch 14.1 - Scanner Core
- [x] Add the `attention-markers` scan command and argument parsing.
- [x] Implement marker detection over the existing scan traversal pipeline.
- [x] Count `scanned-files`, `matched-lines`, and `findings`.
- [x] Reuse child-catalog workspace fanout and bounded ignore handling.

### Batch 14.2 - Output and Contracts
- [x] Add text rendering with warning-row suppression and `--show-warnings`.
- [x] Add markdown rendering and file output support.
- [x] Add schema-versioned JSON payloads and CLI envelope coverage.
- [x] Add manifest config support and schema docs for `[scan.attention_markers]`.

### Batch 14.3 - Doctor Integration
- [x] Add `scan.attention-markers` as a doctor-backed check.
- [x] Support doctor opt-out through `[scan.attention_markers].doctor = false`.
- [x] Preserve category/severity evidence in both doctor text and JSON output.

### Batch 14.4 - Documentation and Validation
- [x] Update command docs, manifest cookbook, JSON examples, and snippets.
- [x] Add regression coverage for nested repos, `.gitignore` behavior, and default exclusions.
- [x] Add validation logs for the scan payload and doctor bridge if the feature lands in multiple batches.

## 7) Acceptance Criteria

- [x] `effigy scan attention-markers` finds explicit attention markers in source/test files across nested catalogs.
- [x] Default output is concise in terminal text and complete in markdown/JSON.
- [x] `effigy doctor` includes attention-marker findings when enabled.
- [x] Manifest config can tune marker families, severity bands, and output defaults.
- [x] Help/docs/contracts clearly describe default exclusions and warning-row behavior.

## 8) Risks and Mitigations

- [ ] Risk: noisy findings overwhelm the default terminal output.
  - Mitigation: hide warning rows by default and keep `--show-warnings` as an explicit opt-in.
- [ ] Risk: marker matching catches unrelated prose or docs text.
  - Mitigation: constrain default scope to code/test paths and use explicit marker lists rather than fuzzy inference.
- [ ] Risk: deprecation matching becomes language-fragile.
  - Mitigation: support common literal forms in v1 and leave AST-aware enrichment for a later roadmap.
- [ ] Risk: scanner drift between `scan` and `doctor`.
  - Mitigation: keep one scanner core and make doctor a thin wrapper around it.

## 9) Deliverables

- [x] `effigy scan attention-markers`
- [x] `[scan.attention_markers]` manifest contract
- [x] `doctor` integration for `scan.attention-markers`
- [x] text/markdown/JSON contract coverage
- [x] updated command/config/docs coverage

## 10) Validation

- [x] `cargo test run_manifest_task_builtin_scan_ --lib`
- [x] `cargo test scan_contract_tests --lib`
- [x] `cargo test doctor_json_contract_ --lib`
- [x] `cargo test cli_json_mode_scan_attention_markers_ --test cli_output_tests`
- [x] `bash docs/scripts/check-vision-metadata.sh`
- [x] Validation logs:
  - `docs/logs/archive/2026-03/06-091500-scan-attention-markers-envelope-and-doctor-validation.md`

## 11) Next Task

Open `g01.015` for the next scan-family milestone or adjacent health-scanner follow-up.
