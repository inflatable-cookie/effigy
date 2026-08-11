# 052 - Changelog Workflow

Use this guide when you want to understand Effigy's changelog contract as both
a CLI workflow and a reusable library surface.

This guide is narrower than the release guides.

Use:
- this guide for `effigy changelog ...`
- [`051-release-orchestration.md`](./051-release-orchestration.md) for the
  actual release cut flow
- [`049-ci-binary-distribution-and-release-protocol.md`](./049-ci-binary-distribution-and-release-protocol.md)
  for maintainer release policy

## Vision Alignment

- Primary tags: `CONTRACT`, `RELEASE`, `OPERATE`
- Target movement: changelog handling becomes one documented, testable Effigy
  surface instead of a mix of ad-hoc markdown editing and release-note scripts.

## Start Here

Use this guide when the changelog should become part of the release contract
instead of a loosely managed markdown file.

Start with the command that matches the job in front of you:

```bash
effigy changelog validate CHANGELOG.md
effigy changelog analyze CHANGELOG.md
effigy changelog extract CHANGELOG.md --version X.Y.Z
```

Quick chooser:

- use `validate` while editing changelog structure
- use `format` when layout drift has accumulated
- use `analyze` before release prep to inspect the next suggested bump
- use `extract` after selecting or cutting a release to generate release-note
  source material

## 1) What Effigy Supports

Effigy ships a built-in changelog surface for four jobs:

- validate changelog structure and policy compliance
- normalize changelog layout into canonical Northstar form
- analyze `Unreleased` entries to suggest the next semver bump
- extract one release body for release notes or GitHub Release publishing

CLI entrypoints:

```bash
effigy changelog validate [FILE]
effigy changelog format [FILE] [--write]
effigy changelog analyze [FILE]
effigy changelog extract [FILE] --version <VERSION>
```

Default file:
- `CHANGELOG.md`

## 2) Northstar Profile Summary

Effigy uses the Northstar Changelog Profile, a strict subset of Keep a
Changelog intended to be easy to parse and validate automatically.

Expected shape:

```md
# Changelog

## [Unreleased]

### Added
- New capability

### Fixed
- Correct behavior

## [0.2.5] - 2026-03-11

### Changed
- Prior release note
```

Key rules:

- top-level title is `# Changelog`
- releases use `## [Unreleased]` or `## [X.Y.Z] - YYYY-MM-DD`
- categories use fixed headings such as `Breaking`, `Added`, `Changed`,
  `Deprecated`, `Removed`, `Fixed`, `Security`
- entries are bullet items
- empty or malformed sections are treated as diagnostics, not ignored silently

## 3) Command Workflows

### Validate

Use this before release preparation or when editing changelog structure:

```bash
effigy changelog validate CHANGELOG.md
```

Use it to catch:

- malformed release headings
- duplicate or out-of-order versions
- invalid dates
- empty sections
- missing `Unreleased`

### Format

Use this when you want canonical layout:

```bash
effigy changelog format CHANGELOG.md
effigy changelog format CHANGELOG.md --write
```

Formatting behavior:

- removes empty categories
- normalizes blank-line spacing
- preserves entry content
- sorts categories into canonical order

### Analyze

Use this to inspect the current `Unreleased` section and the next likely bump:

```bash
effigy changelog analyze CHANGELOG.md
effigy --json changelog analyze CHANGELOG.md
```

Semver behavior:

- post-`1.0.0`:
  - `Breaking` -> major
  - `Added` without `Breaking` -> minor
  - any other non-empty release -> patch
- pre-`1.0.0`:
  - `Breaking` -> minor
  - any other non-empty release -> patch

### Extract

Use this to turn one version into release-note source material:

```bash
effigy changelog extract CHANGELOG.md --version 0.2.5
effigy changelog extract CHANGELOG.md --version Unreleased
```

Behavior:

- prints only the body for that version
- keeps category headings and entries
- omits the outer release heading
- exits non-zero if the requested version is missing or empty

## 4) Release Integration

The built-in release workflow consumes changelog state directly.

Key integration points:

- `effigy release status`
  - validates changelog structure and checks whether `Unreleased` is empty
- `effigy release simulate`
  - previews the bump implied by `Unreleased`
- `effigy release prepare`
  - promotes `Unreleased` into the selected release version and keeps
    `Unreleased` present for future work
- `.github/workflows/release-binaries.yml`
  - uses `effigy changelog extract` to generate GitHub Release notes for the
    tagged version

For the full release contract, see:

- [`051-release-orchestration.md`](./051-release-orchestration.md)
- [`049-ci-binary-distribution-and-release-protocol.md`](./049-ci-binary-distribution-and-release-protocol.md)

## 5) Library Surface

The Rust library entrypoint is `effigy::changelog`.

Main functions:

- `changelog::parse`
- `changelog::load`
- `changelog::validate`
- `changelog::format`
- `changelog::analyze`
- `changelog::extract_version`

Main public types:

- `Changelog`
- `Release`
- `Category`
- `CategoryKind`
- `Analysis`
- `BumpKind`
- `ChangelogError`
- `ParseDiagnostic`
- `ValidationDiagnostic`

Use the library when you need Effigy’s changelog rules inside tests, tools, or
other Rust code without shelling out to the CLI.

## 6) Recommended Operator Sequence

When preparing a release:

First prove the clean pushed candidate SHA with the repository's full hosted
CI board. For Effigy, dispatch `ci.yml` manually and watch the matching run to
success. Then continue:

```bash
effigy changelog validate CHANGELOG.md
effigy changelog analyze CHANGELOG.md
effigy release status --check-gates
effigy release prepare --plan
```

When drafting release notes after a tag is cut:

```bash
effigy changelog extract CHANGELOG.md --version X.Y.Z
```

Then wrap the extracted body with the human-authored note template from guide
`036`.

## Expected Outcome

- changelog structure is explicit and machine-checked
- release-note extraction is repeatable and no longer shell-script specific
- maintainers have one canonical guide for both changelog CLI use and library
  concepts

## Related Guides

- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`036-release-notes-authoring-template-and-examples.md`](./036-release-notes-authoring-template-and-examples.md)
- [`049-ci-binary-distribution-and-release-protocol.md`](./049-ci-binary-distribution-and-release-protocol.md)
- [`051-release-orchestration.md`](./051-release-orchestration.md)

## Next Step

Use `effigy changelog analyze CHANGELOG.md` before the next release-prep batch,
and use `effigy changelog extract CHANGELOG.md --version X.Y.Z` as the first
draft source for release notes after tagging.
