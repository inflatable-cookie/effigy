# 018 - Generated In Src Scan And Doctor Integration

Generation: `g01`

Status: Complete
Owner: Platform
Created: 2026-03-06
Depends on: 009, 012, 014, 016, 017

## Vision Alignment

This roadmap expands Effigy's scan and doctor system so generated artefacts that have landed inside source trees can be surfaced early, before they pollute implementation directories and distort repo health signals.

## Primary Tags

- `OPERATE`
- `CONTRACT`
- `MAINT`

## Target Envelope

- Effigy can detect generated files committed inside source-oriented directories and surface them consistently through both `scan` and `doctor`.

## Vision Target Delta

- Move from generic generated-asset and code-shape scanning toward a tighter hygiene policy that distinguishes acceptable generated outputs from generated files embedded in implementation trees.

## 1) Problem

Effigy can already identify bulky generated assets, oversized code files, duplicate blocks, commentary-heavy files, and explicit attention markers, but it still lacks a focused answer to a common hygiene problem: generated files have been checked into `src/`, `app/`, `lib/`, `crates/`, or other implementation trees where humans expect maintained source.

In practice this creates several concrete costs:
- reviews become noisy because machine-produced files sit beside maintained code,
- `god-files`, duplication, and attention scanners spend time traversing files that should never have been in source trees,
- teams stop trusting directory boundaries because implementation paths start containing generated outputs,
- AI-assisted workflows often drop generated clients, snapshots, or codegen output into `src/` without anyone noticing until cleanup is expensive.

Without a dedicated scanner, Effigy can tell that generated content exists, but not whether it is violating expected source-tree boundaries.

## 2) Goals

- [x] Add `effigy scan generated-in-src` as a first-class built-in scanner.
- [x] Detect generated files located inside implementation-oriented directories.
- [x] Reuse the current scan traversal model, including child-catalog fanout and `.gitignore` handling.
- [x] Support text, markdown, and JSON output with stable schema versioning.
- [x] Integrate generated-in-src findings into `effigy doctor`.
- [x] Make source-tree heuristics, generated markers, and traversal rules configurable via `effigy.toml`.

## 3) Non-Goals

- [ ] No generic generated-file inventory across the whole repo in v1.
- [ ] No AST parsing or language-specific codegen provenance detection.
- [ ] No automatic remediation or file moving.
- [ ] No attempt to decide whether generated files are acceptable build outputs outside source trees.
- [ ] No enforcement of package-specific ownership policies beyond path heuristics.

## 4) Scanner Contract

Command surface:
- `effigy scan generated-in-src`
- `effigy scan generated-in-src --show-warnings`
- `effigy scan generated-in-src --markdown --out reports/generated-in-src.md`
- `effigy --json scan generated-in-src`

Core finding model:
- `path`
- `category`
- `severity`
- `reason`
- `size_bytes`

Top-level metrics:
- `scanned-files`
- `candidate-files`
- `findings`

Default severity direction:
- `warning`: generated file in a source tree by marker or filename heuristic
- `high`: generated file in a source tree with strong generated markers or generated bundle/map/minified patterns
- `critical`: generated file in a source tree that is both strongly generated and materially large

Default scope:
- scan code-oriented trees such as `src/`, `app/`, `lib/`, `crates/`, `packages/*/src`, `services/*/src`
- exclude docs, generated artefacts outside source trees, migrations, fixtures, examples, benchmarks, lockfiles
- respect `.gitignore`
- fan out across child catalogs/sub-repos like the existing scanners

Default generated-file heuristics:
- content markers such as `@generated`, `Code generated`, `GENERATED FILE`, `DO NOT EDIT`
- filename/path markers such as `.generated.`, `.gen.`, `.designer.`, `.pb.`, `.g.`, `.min.`, `.map`
- strongly machine-produced formatting or header conventions where cheap and deterministic

## 5) Manifest Contract

Add scanner configuration under:

```toml
[scan.generated_in_src]
doctor = true
respect_gitignore = true
fail_on_findings = false
include = ["src/**", "app/**", "lib/**", "crates/**"]
exclude = ["vendor/**"]
source_roots = ["src/**", "app/**", "lib/**", "crates/**", "packages/*/src/**"]
warn_bytes = 0
high_bytes = 20000
critical_bytes = 200000
format = "text"
out = "reports/generated-in-src.md"
```

The command allows CLI overrides for:
- output mode (`--json`, `--markdown`, `--out`)
- traversal (`--no-gitignore`, `--include`, `--exclude`)
- reporting verbosity (`--show-warnings`)
- fail mode (`--fail-on-findings`)
- source-tree targeting (`--source-root`)
- size thresholds (`--high`, `--critical`)

Initial doctor policy:
- ship doctor integration in the roadmap
- keep `[scan.generated_in_src].doctor = true` as the default unless benchmarked noise says otherwise, because this should be a low-noise, path-scoped hygiene signal

## 6) Detection Strategy

Use a deterministic, bounded path-plus-marker approach in v1:

1. traverse files using the existing workspace scan fanout model
2. restrict candidate evaluation to configured source-root path patterns
3. classify files as generated by content markers, filename conventions, and existing generated heuristics
4. rank severity by generated-signal strength plus file size
5. sort findings by severity, source-root confidence, then size

Expected properties:
- deterministic output
- cheap enough for default `doctor` participation
- significantly lower noise than generic generated-asset scanning
- useful as a companion to `generated-assets`, not a replacement for it

## 7) Execution Plan

### Batch 18.1 - Scanner Core
- [x] Add the `generated-in-src` scan command and argument parsing.
- [x] Implement source-root path filtering and generated-file classification on the existing traversal pipeline.
- [x] Count `scanned-files`, `candidate-files`, and `findings`.
- [x] Reuse child-catalog workspace fanout and bounded ignore handling.

### Batch 18.2 - Output and Contracts
- [x] Add text rendering with warning-row suppression and `--show-warnings`.
- [x] Add markdown rendering and file output support.
- [x] Add schema-versioned JSON payloads and CLI envelope coverage.
- [x] Add manifest config support and schema docs for `[scan.generated_in_src]`.

### Batch 18.3 - Doctor Integration
- [x] Add `scan.generated-in-src` as a doctor-backed check.
- [x] Support doctor participation through `[scan.generated_in_src].doctor = true`.
- [x] Summarize generated-in-src findings in doctor text and write file-level details to `.effigy/reports/doctor/scan-generated-in-src.md`.

### Batch 18.4 - Documentation and Validation
- [x] Update command docs, manifest cookbook, JSON examples, and snippets.
- [x] Add regression coverage for nested repos, `.gitignore` behavior, and source-root defaults.
- [x] Benchmark a real repo such as `example-app` to confirm runtime and default doctor signal quality.

## 8) Acceptance Criteria

- [x] `effigy scan generated-in-src` finds generated files inside nested source trees across child catalogs.
- [x] Default output is concise in terminal text and complete in markdown/JSON.
- [x] `effigy doctor` includes generated-in-src findings by default unless explicitly disabled.
- [x] Manifest config can tune source-root rules, severity thresholds, and output defaults.
- [x] Help/docs/contracts clearly describe source-root targeting, generated markers, and its relationship to `generated-assets`.

## 9) Risks and Mitigations

- [ ] Risk: false positives on legitimately maintained files that happen to match generator-like names.
  - Mitigation: require either strong markers or a combination of path and generated heuristics; keep config escape hatches.
- [ ] Risk: overlap with `generated-assets` confuses users.
  - Mitigation: document this scan as a boundary-violation check for source trees, while `generated-assets` remains repo inflation detection.
- [ ] Risk: language/client generators produce many acceptable checked-in files.
  - Mitigation: allow source-root narrowing and per-repo exclusions without disabling the scanner entirely.
- [ ] Risk: nested repo path resolution causes parent workspace rules to leak.
  - Mitigation: reuse the bounded ignore and child-catalog fanout model already validated in other scanners.

## 10) Deliverables

- [x] `effigy scan generated-in-src`
- [x] `[scan.generated_in_src]` manifest contract
- [x] `doctor` integration for `scan.generated-in-src`
- [x] text/markdown/JSON contract coverage
- [x] updated command/config/docs coverage

## 11) Validation

- [x] `cargo test run_manifest_task_builtin_scan_generated_in_src_ --lib`
- [x] `cargo test doctor_json_contract_ --lib`
- [x] `cargo test doctor_check_registry_ --lib`
- [x] `cargo test cli_json_mode_scan_generated_in_src_ --test cli_output_tests`
- [x] `bash docs/scripts/check-vision-metadata.sh`
- [x] benchmark logs for at least one real repo before changing default doctor policy

Benchmark summary:
- `example-app` standalone `scan generated-in-src` run: `scanned-files=1716`, `candidate-files=4`, `findings=4`, `real=2.06s`
- Decision: keep `[scan.generated_in_src].doctor = true` as the default because runtime and noise are acceptable for default health runs.

## 12) Next Task

Roadmap `g01.018` is complete. Reassess the next scan milestone and decide whether the next target should stay hygiene-focused (`generated-assets` refinements, `stale-suppressions`) or move toward deeper structural/policy checks (`dead-exports`, `layering`).
