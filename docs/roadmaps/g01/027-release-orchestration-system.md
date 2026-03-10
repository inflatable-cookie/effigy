# 027 - Release Orchestration System

Generation: `g01`

Status: Planned
Owner: Platform
Created: 2026-03-10
Depends on: 026

## Vision Alignment

This roadmap implements release orchestration as a first-class Effigy feature —
on the same level as testing orchestration and task invocation. Any project
using Effigy can declare a release configuration and execute human-gated
releases through a consistent, auditable workflow. The system is built _in_
Effigy, validated _on_ Effigy's own releases, and designed _for_ every project
in the ecosystem.

## Primary Tags

- `RELEASE`
- `OPERATE`
- `MAINT`

## Target Envelope

- Effigy provides `release` subcommands (`status`, `prepare`, `execute`,
  `simulate`) as built-in features.
- Release configuration lives in `effigy.toml` alongside task definitions.
- The system enforces a changelog-first, human-gated workflow: version is
  computed from changelog content, not from commit messages.
- Human approval is required at every decision point. Agents propose; humans
  approve.
- Effigy's own release process (`prepare-release.sh`, `check-release-gates.sh`)
  is migrated onto this system, proving the design.
- Other projects adopt the same `effigy release prepare` workflow without
  custom scripts.

## Vision Target Delta

- Moved from `per-project shell scripts for release management` toward
  `declarative release orchestration with human gates, changelog-first
  versioning, and consistent cross-project workflow`.

## Design Principles

### 1. Changelog-First Versioning

The version number is _derived from_ the changelog, not _written into_ it.
The [Unreleased] section is the source of truth for what the next release
contains. The system analyzes those entries and proposes a version bump.

This is the opposite of commit-driven tools (Semantic Release, Release Please)
where commits determine the version. Here, the human curates the changelog and
the system proposes accordingly.

### 2. Human-Gated Workflow

Every consequential action requires explicit human approval:

```
Agent proposes version  →  Human approves version
Agent formats changelog →  Human approves formatting
Agent runs gates        →  Human reviews gate results
Agent shows summary     →  Human says "execute"
Agent commits and tags  →  (only after all approvals)
```

No tag is ever created without the human explicitly saying "execute". This is a
hard safety invariant, not a soft default.

### 3. Feature Parity with Task Runner

Release orchestration is a first-class feature alongside:
- **Task invocation** (`effigy run`, `effigy test`)
- **Testing orchestration** (`effigy test --plan`, test selection, parallel runs)
- **Doctor/health checks** (`effigy doctor`)

It is not a plugin, not an external tool, not a script. It ships with Effigy
and is configured in `effigy.toml`.

### 4. Progressive Adoption

Projects adopt release orchestration incrementally:
1. Start with `effigy changelog validate` (from roadmap 026)
2. Add `[release]` config to `effigy.toml`
3. Use `effigy release status` to check readiness
4. Use `effigy release prepare` for the full workflow
5. Retire project-specific release scripts

---

## 1) Release Configuration Schema

Define the `[release]` section in `effigy.toml`.

Location: `src/config/release.rs`

```toml
[release]
# Version source - where to read/write the current version
version-file = "Cargo.toml"          # also supports package.json, pyproject.toml
version-path = "package.version"     # JSON path or TOML path to version field

# Changelog
changelog = "CHANGELOG.md"

# Version policy
pre-1-0 = true                       # treat Breaking as MINOR (not MAJOR)

# Files to update on version bump (beyond version-file)
sync-files = ["Cargo.lock"]          # files regenerated after version change

# Release gates (commands that must pass before release)
[release.gates]
format = "cargo fmt --all -- --check"
lint = "cargo clippy --workspace -- -D warnings"
test = "cargo test --workspace"
docs = "cargo doc --workspace --no-deps"

# Optional: custom gate with description
[release.gates.smoke]
command = "./scripts/check-release-smoke.sh"
description = "Smoke test the release binary"

# Tag format
tag-format = "v{version}"            # default: "v{version}"

# Post-release hooks (run after successful tag push)
[release.post]
# These are informational — they document what CI will do
ci-pipeline = "release-binaries.yml triggers on tag push"
```

Tasks:
- [ ] Define `ReleaseConfig` struct with serde deserialization
- [ ] Define `GateConfig` supporting both string shorthand and table form
- [ ] Support `version-file` detection for Cargo.toml, package.json,
  pyproject.toml, and VERSION file
- [ ] Support `version-path` for extracting version from structured files
- [ ] Validate configuration on load (file exists, gates are valid commands)
- [ ] Provide sensible defaults for Rust projects (detect Cargo.toml
  automatically)

## 2) Version File Operations

Implement reading and writing version numbers across different file formats.

Location: `src/release/version.rs`

Supported version file formats:
- `Cargo.toml` — TOML `[package] version = "X.Y.Z"`
- `package.json` — JSON `"version": "X.Y.Z"`
- `pyproject.toml` — TOML `[project] version = "X.Y.Z"` or
  `[tool.poetry] version`
- `VERSION` — plain text file containing just the version string

Tasks:
- [ ] Implement `read_version(file: &Path, path: &str) -> Result<Version>`
- [ ] Implement `write_version(file: &Path, path: &str, version: &Version) -> Result<()>`
- [ ] TOML reader/writer that preserves formatting and comments
- [ ] JSON reader/writer that preserves formatting
- [ ] Plain text reader/writer for VERSION files
- [ ] Auto-detect version file if not configured (search for Cargo.toml,
  package.json, etc.)
- [ ] Implement `sync_files` execution (e.g., `cargo generate-lockfile` after
  Cargo.toml version change)

## 3) Release Status Command

Implement `effigy release status` — a non-destructive readiness check.

Location: `src/release/status.rs`

Output:
```
Release Status
  Current version: 0.2.4
  Unreleased changes: 3 (1 Added, 2 Fixed)
  Suggested bump: patch → 0.2.5
  Changelog: valid
  Gates: not checked (run with --check-gates)
  Ready: yes (pending gate validation)
```

Tasks:
- [ ] Load release config from `effigy.toml`
- [ ] Read current version from version file
- [ ] Analyze changelog for unreleased entries (uses `crates/changelog`)
- [ ] Compute suggested version bump
- [ ] Validate changelog format
- [ ] Optional `--check-gates` flag to run gate commands
- [ ] JSON output with `--format=json` for scripting
- [ ] Exit code: 0 if ready, 1 if blockers exist

## 4) Release Prepare Command (Interactive Workflow)

Implement `effigy release prepare` — the human-gated release preparation
workflow.

Location: `src/release/prepare.rs`

This is the core of the system. It runs an interactive workflow with multiple
approval points.

### Step-by-step flow:

```
Step 1: Validate changelog
  → Show validation results
  → If errors: stop and report (human must fix)

Step 2: Analyze changes and propose version
  → Show unreleased entry summary
  → Show suggested bump with reasoning
  → HUMAN APPROVAL: "Accept version X.Y.Z? [y/N/custom]"

Step 3: Format changelog
  → Show diff of formatting changes
  → HUMAN APPROVAL: "Apply formatting? [y/N]"

Step 4: Update version files
  → Show list of files to be modified
  → Show the version change (old → new)
  → HUMAN APPROVAL: "Update version files? [y/N]"

Step 5: Run release gates
  → Execute each gate command sequentially
  → Show pass/fail for each gate
  → If any fail: stop (human must fix and re-run)

Step 6: Present summary
  → Show complete summary: version, changelog diff, files changed, gate
    results
  → HUMAN APPROVAL: "Ready to execute release? [y/N]"
  → If approved: write a `.release-prepared.json` state file
```

The prepare command does NOT commit, tag, or push. It stages all changes and
writes a state file that the execute command reads.

State file (`.release-prepared.json`):
```json
{
  "version": "0.2.5",
  "previous_version": "0.2.4",
  "tag": "v0.2.5",
  "prepared_at": "2026-03-10T14:30:00Z",
  "gates_passed": true,
  "files_modified": ["Cargo.toml", "Cargo.lock", "CHANGELOG.md"],
  "changelog_section": "### Fixed\n- Fix widget alignment..."
}
```

Tasks:
- [ ] Implement step 1: changelog validation (reuse from `crates/changelog`)
- [ ] Implement step 2: version proposal with interactive approval
- [ ] Implement step 3: changelog formatting with diff display
- [ ] Implement step 4: version file updates with preview
- [ ] Implement step 5: gate execution with sequential runs and reporting
- [ ] Implement step 6: summary and final approval
- [ ] Implement `.release-prepared.json` state file writing
- [ ] Implement `--dry-run` flag that shows what would happen without prompts
- [ ] Handle re-running after partial preparation (detect existing state)
- [ ] Support non-interactive mode for CI (`--yes` flag with all approvals
  pre-given — still requires state file for execute)

## 5) Release Execute Command

Implement `effigy release execute` — commits, tags, and pushes.

Location: `src/release/execute.rs`

This command reads the state file from prepare and performs the irreversible
actions.

### Flow:

```
Step 1: Validate state file exists and is recent
  → Error if no .release-prepared.json
  → Warning if prepared more than 1 hour ago (stale state)

Step 2: Verify working tree matches expectations
  → Check that the files listed in state file are modified
  → Check that no unexpected changes are present

Step 3: FINAL HUMAN APPROVAL
  → "This will commit, tag v0.2.5, and push. Continue? [y/N]"

Step 4: Commit
  → `git add` the modified files
  → `git commit -m "release: vX.Y.Z"`

Step 5: Tag
  → `git tag vX.Y.Z`

Step 6: Push
  → `git push origin main --tags`
  → Show CI pipeline URL if detectable

Step 7: Clean up
  → Remove .release-prepared.json
  → Print post-release checklist
```

Tasks:
- [ ] Implement state file loading and validation
- [ ] Implement staleness check (configurable threshold, default 1 hour)
- [ ] Implement working tree verification
- [ ] Implement final approval prompt
- [ ] Implement git commit with conventional message format
- [ ] Implement git tag creation
- [ ] Implement git push with tag
- [ ] Implement state file cleanup
- [ ] Print post-release monitoring instructions
- [ ] Handle failures gracefully: if push fails, do NOT re-tag

## 6) Release Simulate Command

Implement `effigy release simulate` — full dry-run showing what would happen.

Location: `src/release/simulate.rs`

This is the safe preview mode. It runs the full prepare workflow in read-only
mode, showing every step without modifying any files.

Tasks:
- [ ] Run changelog analysis without modifying files
- [ ] Show proposed version bump
- [ ] Show what formatting changes would be applied
- [ ] Show which files would be modified
- [ ] Run gates (these are read-only by nature)
- [ ] Show the commit message and tag that would be created
- [ ] No state file written
- [ ] JSON output support for scripting

## 7) Gate System

Implement a flexible gate system that validates release readiness.

Location: `src/release/gates.rs`

Gates are commands that must exit 0 for the release to proceed. They run
sequentially (not parallel) so output is clear and ordered.

```
Running release gates...
  [1/5] format        ✓  (0.3s)
  [2/5] lint          ✓  (4.2s)
  [3/5] test          ✓  (12.1s)
  [4/5] docs          ✓  (3.0s)
  [5/5] smoke         ✓  (1.1s)

All 5 gates passed in 20.7s
```

Tasks:
- [ ] Load gate definitions from `[release.gates]` config
- [ ] Execute gates sequentially with output capture
- [ ] Report pass/fail with timing for each gate
- [ ] Stop on first failure (fail-fast)
- [ ] Show captured output for failed gates
- [ ] Support `effigy release gates` standalone command (run gates without
  full prepare workflow)
- [ ] Exit code: 0 if all pass, 1 if any fail

## 8) Effigy Self-Hosting Migration

Migrate Effigy's own release process onto the new system.

This is the critical validation step. Effigy's current release flow uses
`prepare-release.sh` and `check-release-gates.sh`. Those scripts must be
replaced by the new `effigy release` commands.

### Current → New mapping:

| Current | New |
|---------|-----|
| `./scripts/prepare-release.sh` | `effigy release prepare` |
| `./scripts/check-release-gates.sh` | `effigy release gates` |
| Manual `git tag` + `git push` | `effigy release execute` |
| `sed` extraction in release-binaries.yml | `effigy changelog extract` |

### Migration tasks:

- [ ] Add `[release]` section to Effigy's `effigy.toml`
- [ ] Configure gates to match current `check-release-gates.sh` checks
- [ ] Configure version file as `Cargo.toml` with `Cargo.lock` sync
- [ ] Run parallel validation: old scripts and new system produce same results
- [ ] Update `release-binaries.yml` to use `effigy changelog extract` for
  release notes (requires human approval per CLAUDE.md)
- [ ] Update guide 049 (Release Protocol) section 6c to reference new commands
- [ ] Update guide 014 (Release Checklist) to reference new workflow
- [ ] Retire `prepare-release.sh` and `check-release-gates.sh` (keep as
  backup until one successful release with new system)
- [ ] Execute first release using `effigy release prepare` + `effigy release
  execute`

## 9) Cross-Project Adoption Support

Ensure the release system works for projects beyond Effigy.

### Design for variety:

The system must handle:
- **Rust projects** (Cargo.toml, Cargo.lock sync, `cargo test` gates)
- **Node.js projects** (package.json, package-lock.json sync, `npm test` gates)
- **Python projects** (pyproject.toml, `pytest` gates)
- **Multi-language monorepos** (version in VERSION file, heterogeneous gates)

### Validation tasks:

- [ ] Test configuration with a Node.js project version file (package.json)
- [ ] Test configuration with a plain VERSION file
- [ ] Verify gate commands work with non-Rust toolchains
- [ ] Document configuration examples for each project type
- [ ] Add to Effigy's agent adoption guide (047) with release orchestration
  section

## 10) Documentation and Guides

Update Effigy documentation for the new release orchestration feature.

Tasks:
- [ ] Create new guide: `docs/guides/NNN-release-orchestration.md`
  - Configuration reference
  - Workflow walkthrough
  - Gate configuration
  - Version file formats
  - Migration from custom scripts
- [ ] Update guide 049 (CI Binary Distribution and Release Protocol):
  - Section 6c: reference `effigy release` commands
  - Section 7a: update agent protocols for new commands
- [ ] Update guide 014 (Release Checklist): integrate with new workflow
- [ ] Update CLAUDE.md: reference `effigy release` in Release Protocol section
- [ ] Add `effigy release --help` with comprehensive usage text

---

## Completion Criteria

This roadmap is complete when:
1. `effigy release status` reports readiness for any configured project.
2. `effigy release prepare` walks through the full human-gated workflow.
3. `effigy release execute` commits, tags, and pushes after approval.
4. `effigy release simulate` shows the full dry-run without side effects.
5. All gates run and report pass/fail with timing.
6. Effigy's own release process uses the new system (self-hosting).
7. At least one release has been successfully executed using the new system.
8. Documentation is complete with configuration reference and examples.

## Dependencies

- **Roadmap 026** (Changelog Library) — the changelog crate provides parsing,
  formatting, validation, and analysis used throughout the release workflow.
- `semver` — version computation
- `chrono` — timestamps in state files
- `serde` / `serde_json` — state file serialization, JSON output
- `dialoguer` or `inquire` — interactive prompts (evaluate at implementation
  time)

## Phasing Recommendation

This roadmap is large. Recommended execution order:

1. **Release config and status** (sections 1, 3) — immediate value with
   `effigy release status`
2. **Version file ops and gates** (sections 2, 7) — foundational for prepare
3. **Prepare workflow** (section 4) — the core interactive experience
4. **Execute and simulate** (sections 5, 6) — complete the command set
5. **Self-hosting migration** (section 8) — prove the design on Effigy itself
6. **Cross-project and docs** (sections 9, 10) — broaden adoption

Each phase delivers standalone value. Phase 1-2 can be used immediately even
before the full prepare/execute workflow is complete.

## Safety Invariants

These invariants must hold across all implementations:

1. **No tag without human approval.** The execute command must always prompt
   for confirmation before creating a git tag.
2. **No push without human approval.** Pushing to remote is irreversible and
   must be explicitly approved.
3. **Gates must pass.** The prepare workflow must not produce a state file if
   any gate fails.
4. **State file required for execute.** The execute command must refuse to run
   without a valid state file from prepare.
5. **Never re-tag.** If a tagged release fails in CI, the fix goes into the
   next PATCH version. The system must not offer to re-tag.

## Reference Documents

- Agent playbook: `northstar/bundle-docs/research/translation-memos/ai-agent-release-playbook.md`
- Implementation brief: `northstar/bundle-docs/research/handoff/IMPLEMENTATION_BRIEF.md`
- Boundary memo: `northstar/bundle-docs/research/translation-memos/northstar-effigy-boundary.md`
- Current release protocol: `docs/guides/049-ci-binary-distribution-and-release-protocol.md`
- Current release scripts: `scripts/prepare-release.sh`, `scripts/check-release-gates.sh`
