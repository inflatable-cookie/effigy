# 019 - Stale Suppressions Scan And Doctor Integration

Generation: `g01`

Status: Complete
Owner: Platform
Created: 2026-03-06
Depends on: 009, 012, 014, 017, 018

## Vision Alignment

This roadmap expands Effigy's scan and doctor system so suppression markers that hide warnings, lints, type errors, or policy failures can be surfaced before they silently accumulate and make repo health signals untrustworthy.

## Primary Tags

- `OPERATE`
- `CONTRACT`
- `MAINT`

## Target Envelope

- Effigy can detect suppression markers in maintained source files and surface them consistently through both `scan` and `doctor`.

## Vision Target Delta

- Move from general structural and hygiene scanning toward explicit visibility into places where developers have muted tooling, deferred correctness work, or hidden failures behind inline suppressions.

## 1) Problem

Effigy can already identify oversized code files, duplicate blocks, commentary-heavy files, generated files in source trees, and explicit attention markers, but it still lacks a focused answer to another common maintenance problem: suppressions spread across the codebase and quietly weaken tooling guarantees over time.

In practice this creates several concrete costs:
- linter, typechecker, and compiler suppressions accumulate until teams stop trusting “clean” runs,
- AI-generated changes often add broad ignores or disable comments to force progress through failing checks,
- reviewers see local suppressions one file at a time but lack a repo-wide picture of where debt has concentrated,
- doctor can report downstream health issues without exposing the places where those issues have been muted.

Without a dedicated scanner, Effigy can surface some deferred-work markers, but not the concrete suppression mechanisms that directly hide warnings and policy failures.

## 2) Goals

- [ ] Add `effigy scan stale-suppressions` as a first-class built-in scanner.
- [ ] Detect common suppression markers across supported source and test files.
- [ ] Reuse the current scan traversal model, including child-catalog fanout and `.gitignore` handling.
- [ ] Support text, markdown, and JSON output with stable schema versioning.
- [ ] Integrate stale-suppressions findings into `effigy doctor`.
- [ ] Make suppression categories, severities, and traversal rules configurable via `effigy.toml`.

## 3) Non-Goals

- [ ] No semantic proof in v1 that a suppression is definitely unnecessary.
- [ ] No automatic removal or autofix of suppressions.
- [ ] No build-tool invocation to test whether a suppression still matters.
- [ ] No docs-site or markdown scanning by default.
- [ ] No repo-policy language beyond marker-based suppression visibility in v1.

## 4) Scanner Contract

Command surface:
- `effigy scan stale-suppressions`
- `effigy scan stale-suppressions --show-warnings`
- `effigy scan stale-suppressions --markdown --out reports/stale-suppressions.md`
- `effigy --json scan stale-suppressions`

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

Default suppression families:
- Type/language suppressions:
  - `@ts-ignore`
  - `@ts-expect-error`
  - `type: ignore`
  - `type: ignore[...]`
  - `#[allow(...)]`
  - `#[expect(...)]`
- Lint suppressions:
  - `eslint-disable`
  - `eslint-disable-next-line`
  - `nolint`
  - `golangci-lint`
  - `rubocop:disable`
  - `swiftlint:disable`
- Formatter and tool bypasses:
  - `fmt: off`
  - `prettier-ignore`
  - `stylelint-disable`
  - `shellcheck disable=`

Default severity direction:
- `warning`: narrow, line-scoped suppressions such as `eslint-disable-next-line`, `@ts-expect-error`, `type: ignore`
- `high`: broad or file-scoped suppressions such as `eslint-disable`, `#[allow(...)]`, `rubocop:disable`, `swiftlint:disable`
- `critical`: strong “hide everything” patterns or explicit broad bypasses such as `nolint`, `#[allow(warnings)]`, `eslint-disable` with no rule target, or `type: ignore` applied at file scope where detectable

Default scope:
- include source and test files
- exclude docs, generated artefacts, migrations, fixtures, examples, benchmarks, lockfiles
- respect `.gitignore`
- fan out across child catalogs/sub-repos like the existing scanners

Default matching direction:
- marker-based and deterministic
- line-oriented findings with tight snippets
- no attempt in v1 to decide if the suppression is truly obsolete, only that it exists and is broad enough to warrant review

## 5) Manifest Contract

Add scanner configuration under:

```toml
[scan.stale_suppressions]
doctor = false
respect_gitignore = true
fail_on_findings = false
include = ["src/**", "crates/**", "tests/**"]
exclude = ["vendor/**"]
warning = ["@ts-ignore", "@ts-expect-error", "type: ignore", "eslint-disable-next-line"]
high = ["eslint-disable", "#[allow(", "rubocop:disable", "swiftlint:disable"]
critical = ["nolint", "#[allow(warnings)]", "shellcheck disable=", "eslint-disable"]
format = "text"
out = "reports/stale-suppressions.md"
```

The command should allow CLI overrides for:
- output mode (`--json`, `--markdown`, `--out`)
- traversal (`--no-gitignore`, `--include`, `--exclude`)
- reporting verbosity (`--show-warnings`)
- fail mode (`--fail-on-findings`)
- marker families (`--warning-marker`, `--high-marker`, `--critical-marker`)

Initial doctor policy:
- ship doctor integration in the roadmap
- keep `[scan.stale_suppressions].doctor = false` as the default after benchmark validation; the runtime is acceptable, but broad suppression markers create too much default doctor noise in large repos

## 6) Detection Strategy

Use a deterministic, bounded marker-based approach in v1:

1. traverse supported files through the existing workspace scan fanout model
2. classify lines using extension-aware suppression marker families
3. emit one finding per matched line after de-duplicating overlapping marker forms
4. rank severity by marker family and breadth heuristics
5. sort findings by severity, category, then path/line

Expected properties:
- deterministic output
- cheap enough for default `doctor` participation
- easy to explain in terminal output and JSON contracts
- useful as a companion to `attention-markers`, not a replacement for it

## 7) Execution Plan

### Batch 19.1 - Scanner Core
- [x] Add the `stale-suppressions` scan command and argument parsing.
- [x] Implement suppression marker detection across the existing scan traversal pipeline.
- [x] Count `scanned-files`, `matched-lines`, and `findings`.
- [x] Reuse child-catalog workspace fanout and bounded ignore handling.

### Batch 19.2 - Output and Contracts
- [x] Add text rendering with warning-row suppression and `--show-warnings`.
- [x] Add markdown rendering and file output support.
- [x] Add schema-versioned JSON payloads and CLI envelope coverage.
- [x] Add manifest config support and schema docs for `[scan.stale_suppressions]`.

### Batch 19.3 - Doctor Integration
- [x] Add `scan.stale-suppressions` as a doctor-backed check.
- [x] Support doctor participation through `[scan.stale_suppressions].doctor = true`.
- [x] Summarize stale-suppressions findings in doctor text and write file-level details to `.effigy/reports/doctor/scan-stale-suppressions.md`.

### Batch 19.4 - Documentation and Validation
- [x] Update command docs, manifest cookbook, JSON examples, and snippets.
- [x] Add regression coverage for nested repos, `.gitignore` behavior, and marker de-duplication.
- [x] Benchmark a real repo such as `acowtancy` to confirm runtime and default doctor signal quality.

## 8) Acceptance Criteria

- [x] `effigy scan stale-suppressions` finds suppression markers across nested catalogs.
- [x] Default output is concise in terminal text and complete in markdown/JSON.
- [x] `effigy doctor` supports stale-suppressions findings when explicitly enabled.
- [x] Manifest config can tune suppression families, traversal rules, and output defaults.
- [x] Help/docs/contracts clearly describe supported marker families, exclusions, and its relationship to `attention-markers`.

## 9) Risks and Mitigations

- [ ] Risk: repo-specific suppression styles create false negatives.
  - Mitigation: ship a strong default marker set and support manifest/CLI extension points from day one.
- [ ] Risk: broad markers like `eslint-disable` or `allow` create noisy matches without enough context.
  - Mitigation: match exact suppression forms, classify breadth carefully, and keep snippets/path+line visible.
- [ ] Risk: overlap with `attention-markers` confuses users.
  - Mitigation: document this scan as concrete tooling-bypass visibility, while `attention-markers` remains general deferred-work detection.
- [ ] Risk: doctor output becomes noisy in large repos.
  - Mitigation: reuse summary-only doctor text plus detail-file report output.

## 10) Deliverables

- [x] `effigy scan stale-suppressions`
- [x] `[scan.stale_suppressions]` manifest contract
- [x] `doctor` integration for `scan.stale-suppressions`
- [x] text/markdown/JSON contract coverage
- [x] updated command/config/docs coverage

## 11) Validation

- [x] `cargo test run_manifest_task_builtin_scan_ --lib`
- [x] `cargo test scan_contract_tests --lib`
- [x] `cargo test doctor_json_contract_ --lib`
- [x] `cargo test cli_json_mode_scan_ --test cli_output_tests`
- [x] `bash docs/scripts/check-vision-metadata.sh`
- [x] benchmark logs for at least one real repo before confirming default doctor participation

## 12) Outcome

Status: complete

The stale-suppressions scanner is fully landed as a standalone `scan` command with optional doctor integration. Benchmarking on `acowtancy` kept doctor support opt-in by default because the scan is useful but too noisy for routine health runs.
