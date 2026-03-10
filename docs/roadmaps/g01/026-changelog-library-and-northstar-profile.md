# 026 - Changelog Library and Northstar Profile

Generation: `g01`

Status: Planned
Owner: Platform
Created: 2026-03-10
Depends on: 024

## Vision Alignment

This roadmap implements a changelog parsing, formatting, validation, and
analysis library in Effigy, following the Northstar Changelog Profile — a strict
subset of Keep a Changelog 1.0.0. The format specification lives in Northstar
(defining the contract), and the implementation lives in Effigy (providing the
tooling). This is the foundation for automated release management across the
ecosystem.

## Primary Tags

- `RELEASE`
- `MAINT`

## Target Envelope

- Effigy ships a `changelog` crate that parses, formats, validates, and analyzes
  changelogs conforming to the Northstar Changelog Profile.
- Northstar documents the profile specification as an ecosystem-wide standard.
- Effigy's own CHANGELOG.md is migrated to comply with the Northstar Profile.
- CLI subcommands expose changelog operations for scripts and CI.
- The library provides the data foundation for roadmap 027 (release
  orchestration).

## Vision Target Delta

- Moved from `ad-hoc changelog maintenance with empty sections and inconsistent
  spacing` toward `machine-parseable changelogs with automated validation,
  formatting, and version analysis`.

## Cross-Project Scope

This roadmap spans two repositories:

| Repository | Responsibility |
|------------|---------------|
| **Northstar** | Defines the Northstar Changelog Profile specification |
| **Effigy** | Implements the changelog library and CLI tooling |

Northstar is the authority on _what_ the format is. Effigy is the authority on
_how_ to work with it.

---

## 1) Northstar Profile Specification (Northstar repo)

Publish the Northstar Changelog Profile as a formal specification in the
Northstar documentation.

Location: Northstar repo — exact path TBD based on Northstar doc conventions

The specification defines:
- Fixed category set: Breaking, Added, Changed, Deprecated, Removed, Fixed,
  Security
- Strict header format: `## [X.Y.Z] - YYYY-MM-DD` (ISO 8601)
- Unreleased header: `## [Unreleased]`
- Entry format: `- description` (list items only, no sub-lists)
- Empty section policy: omit sections with no entries (never include empty
  sections)
- Spacing: exactly one blank line between sections
- File preamble: title line (`# Changelog`) followed by optional description
  paragraph
- Link references at end of file (Keep a Changelog convention)

Source material: `northstar/bundle-docs/research/specifications/northstar-changelog-profile.md`

Tasks:
- [ ] Review and finalize the profile specification from research
- [ ] Publish specification in Northstar's documentation structure
- [ ] Add cross-reference from Effigy docs to the Northstar specification

## 2) Changelog AST and Data Types (Effigy crate)

Define the in-memory representation of a parsed changelog.

Location: `crates/changelog/src/types.rs`

```rust
pub struct Changelog {
    pub title: String,
    pub description: Option<String>,
    pub releases: Vec<Release>,
    pub links: Vec<LinkReference>,
}

pub struct Release {
    pub version: Option<Version>,  // None = Unreleased
    pub date: Option<NaiveDate>,
    pub categories: Vec<Category>,
}

pub struct Category {
    pub kind: CategoryKind,
    pub entries: Vec<Entry>,
}

pub enum CategoryKind {
    Breaking,
    Added,
    Changed,
    Deprecated,
    Removed,
    Fixed,
    Security,
}

pub struct Entry {
    pub description: String,
    pub continuation_lines: Vec<String>,  // multi-line entries
}

pub struct LinkReference {
    pub label: String,
    pub url: String,
}
```

Tasks:
- [ ] Create `crates/changelog/` crate in Effigy workspace
- [ ] Add crate to workspace `Cargo.toml`
- [ ] Define `Changelog`, `Release`, `Category`, `Entry` types
- [ ] Define `CategoryKind` enum with ordering (Breaking first, Security last)
- [ ] Define `LinkReference` for footer links
- [ ] Implement `Display` for `CategoryKind` (maps to section headers)
- [ ] Add `semver` dependency for version handling

## 3) Line-Based Parser

Implement a line-based state machine parser for changelog files.

Location: `crates/changelog/src/parser.rs`

The parser processes the file line-by-line through states:
- `Preamble` - title and description
- `ReleaseHeader` - `## [X.Y.Z] - YYYY-MM-DD` or `## [Unreleased]`
- `CategoryHeader` - `### Breaking`, `### Added`, etc.
- `Entry` - `- description` with optional continuation lines
- `LinkReference` - `[X.Y.Z]: https://...`

Design choice: hand-written state machine over parser combinators. The format
is simple and line-oriented; a state machine produces better error messages
with exact line numbers.

Tasks:
- [ ] Implement `parse(content: &str) -> Result<Changelog, Vec<ParseError>>`
- [ ] Implement state machine with line-by-line processing
- [ ] Parse version headers extracting version and date
- [ ] Parse category headers validating against `CategoryKind`
- [ ] Parse entries including multi-line continuation (indented lines)
- [ ] Parse link references at file end
- [ ] Produce `ParseError` with line number and descriptive message
- [ ] Handle edge cases: empty files, files with only preamble, trailing
  whitespace
- [ ] Test with Effigy's CHANGELOG.md as primary fixture

## 4) Formatter

Implement changelog formatting that normalizes a parsed changelog back to
canonical form.

Location: `crates/changelog/src/formatter.rs`

Formatting rules:
- Remove empty sections (categories with no entries)
- Remove empty releases (releases with no categories after empty-section
  removal) — except Unreleased which is always retained
- Normalize spacing: exactly one blank line between sections
- Preserve entry content verbatim (no rewrapping)
- Maintain category ordering: Breaking, Added, Changed, Deprecated, Removed,
  Fixed, Security
- Maintain release ordering: Unreleased first, then newest-to-oldest
- Preserve link references at end of file

Tasks:
- [ ] Implement `format(changelog: &Changelog) -> String`
- [ ] Implement empty section removal
- [ ] Implement spacing normalization
- [ ] Implement category ordering enforcement
- [ ] Implement `--preview` mode (print to stdout)
- [ ] Implement `--write` mode (update file in place)
- [ ] Roundtrip test: `parse(format(parse(input))) == parse(input)`

## 5) Validator

Implement Northstar Profile compliance validation.

Location: `crates/changelog/src/validator.rs`

Validation checks:
- Category names must be from the fixed set
- Version headers must match `## [X.Y.Z] - YYYY-MM-DD` format
- Dates must be valid ISO 8601
- Versions must be valid semver
- No empty sections present
- Spacing must be normalized
- Entries must start with `- `
- No duplicate version numbers

Tasks:
- [ ] Implement `validate(changelog: &Changelog) -> Vec<ValidationError>`
- [ ] Validate category names against allowed set
- [ ] Validate version header format
- [ ] Validate date format and validity
- [ ] Validate semver compliance
- [ ] Check for empty sections
- [ ] Check for spacing violations (requires raw text access)
- [ ] Check for duplicate versions
- [ ] `ValidationError` includes line number, rule name, and fix suggestion
- [ ] Exit code 0 for compliant, exit code 1 for violations

## 6) Analyzer

Implement changelog analysis for version bump suggestions.

Location: `crates/changelog/src/analyzer.rs`

Analysis output:
- Count of entries per category in Unreleased
- Whether Unreleased is empty
- Suggested version bump based on category presence:
  - Breaking entries present and version >= 1.0.0 → MAJOR
  - Breaking entries present and version < 1.0.0 → MINOR
  - Added/Changed/Deprecated/Removed entries → MINOR (or PATCH if pre-1.0)
  - Fixed/Security only → PATCH
  - Empty → no bump needed

```rust
pub struct Analysis {
    pub unreleased: UnreleasedSummary,
    pub suggested_bump: BumpKind,
    pub current_version: Option<Version>,
    pub next_version: Option<Version>,
}
```

Tasks:
- [ ] Implement `analyze(changelog: &Changelog) -> Analysis`
- [ ] Count entries per category in Unreleased section
- [ ] Implement version bump logic (pre-1.0 rules vs post-1.0 rules)
- [ ] Compute next version from current + suggested bump
- [ ] JSON output support for scripting: `--format=json`
- [ ] Test bump logic against Effigy's changelog history

## 7) Version Extraction

Implement extraction of release notes for a specific version.

Location: `crates/changelog/src/extract.rs`

Tasks:
- [ ] Implement `extract_version(changelog: &Changelog, version: &str) -> Option<String>`
- [ ] Extract renders the version's categories and entries as markdown
- [ ] Support extracting Unreleased section
- [ ] Integrate with GitHub Release notes (replaces `sed` extraction in
  `release-binaries.yml`)

## 8) CLI Interface

Expose changelog operations as CLI subcommands.

Location: `crates/changelog/src/cli.rs` (or integrated into main effigy binary)

```bash
# Validate against Northstar Profile
effigy changelog validate [file]
# Exit 0: compliant. Exit 1: violations with line numbers.

# Format (normalize)
effigy changelog format [file] [--write|--preview]
# --preview: print to stdout. --write: update in place.

# Analyze unreleased changes
effigy changelog analyze [file] [--format=json]
# Show entry counts, suggested bump, next version.

# Extract version notes
effigy changelog extract [file] --version X.Y.Z
# Output markdown for that version.
```

Tasks:
- [ ] Add `changelog` subcommand group to Effigy CLI
- [ ] Implement `validate` subcommand with exit codes
- [ ] Implement `format` subcommand with `--write` and `--preview` flags
- [ ] Implement `analyze` subcommand with JSON output option
- [ ] Implement `extract` subcommand with `--version` flag
- [ ] Default file path to `CHANGELOG.md` in current directory
- [ ] Add to `effigy.toml` task definitions for self-hosting

## 9) Effigy CHANGELOG Migration

Migrate Effigy's existing CHANGELOG.md to comply with the Northstar Profile.

Tasks:
- [ ] Remove all empty sections from existing changelog entries
- [ ] Normalize spacing between sections (exactly one blank line)
- [ ] Verify all category names match the fixed set
- [ ] Verify all version headers match the strict format
- [ ] Add link references at end of file
- [ ] Run `effigy changelog validate` to confirm compliance
- [ ] Commit migration as a standalone change

## 10) Effigy Integration Tasks

Wire the changelog library into Effigy's existing workflows.

Tasks:
- [ ] Add `effigy.toml` task entries:
  - `changelog:validate` - validate changelog format
  - `changelog:format` - format changelog
  - `changelog:analyze` - analyze unreleased changes
- [ ] Add changelog validation to CI (fail PR if changelog is malformed)
- [ ] Replace `sed`-based changelog extraction in `release-binaries.yml` with
  `effigy changelog extract`
- [ ] Update `prepare-release.sh` to use `effigy changelog analyze` for bump
  recommendation
- [ ] Document changelog conventions in developer guide

## Completion Criteria

This roadmap is complete when:
1. `crates/changelog` parses Effigy's entire CHANGELOG.md history.
2. Roundtrip: `parse` then `format` produces equivalent normalized content.
3. Validator catches format violations with line numbers.
4. Analyzer correctly suggests version bumps from changelog content.
5. CLI subcommands work for validate, format, analyze, and extract.
6. Effigy's CHANGELOG.md passes `effigy changelog validate`.
7. Northstar Profile specification is published in Northstar docs.
8. CI validates changelog format on PRs.

## Dependencies

- `semver` - version parsing and comparison
- `chrono` - date parsing
- `clap` - CLI argument parsing (already in Effigy)
- `serde` / `serde_json` - JSON output for analyze command

## Reference Documents

- Northstar research: `northstar/bundle-docs/research/specifications/northstar-changelog-profile.md`
- Formatter spec: `northstar/bundle-docs/research/specifications/changelog-formatter-spec.md`
- Implementation brief: `northstar/bundle-docs/research/handoff/IMPLEMENTATION_BRIEF.md`
- Decisions log: `northstar/bundle-docs/research/DECISIONS.md`
- Boundary memo: `northstar/bundle-docs/research/translation-memos/northstar-effigy-boundary.md`
