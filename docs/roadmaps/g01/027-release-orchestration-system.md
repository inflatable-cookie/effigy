# 027 - Release Orchestration System

Generation: `g01`

Status: Complete
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

Implementation status (2026-04-17):
- The built-in command surface, release config, version-file mutation flow,
  gate execution, prepare/execute state handling, simulate previews, install
  verification, compatibility wrappers, and regression coverage are now
  shipped in the codebase.
- Zero-risk local rehearsal and hosted GitHub rehearsal have now both completed
  successfully, including real tag-triggered `Release Binaries` execution and
  follow-up hosted `CI` / `JSON Contracts` validation for the final
  release-command changes.
- `.github/workflows/release-binaries.yml` has now been cut over from legacy
  inline `sed` release-note extraction to built-in
  `effigy changelog extract`, with hosted validation proving the tagged release
  path publishes changelog-derived notes correctly.
- The remaining unchecked items from the original roadmap are now closed too:
  the first real production Effigy release through the built-in workflow was
  recorded for `v0.2.5`, the workflow cutover to built-in changelog extract
  landed, and the legacy release compatibility wrappers were later retired.

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
- Effigy's own release process (`effigy release prepare`, `effigy release
  gates`) is migrated onto this system, proving the design.
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
command = "cargo run --bin effigy -- smoke:release"
description = "Smoke test the release binary"

# Tag format
tag-format = "v{version}"            # default: "v{version}"

# Post-release hooks (run after successful tag push)
[release.post]
# These are informational — they document what CI will do
ci-pipeline = "release-binaries.yml triggers on tag push"
```

Tasks:
- [x] Define `ReleaseConfig` struct with serde deserialization
- [x] Define `GateConfig` supporting both string shorthand and table form
- [x] Support `version-file` detection for Cargo.toml, package.json,
  pyproject.toml, and VERSION file
- [x] Support `version-path` for extracting version from structured files
- [x] Validate configuration on load (file exists, gates are valid commands)
- [x] Provide sensible defaults for Rust projects (detect Cargo.toml
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
- [x] Implement `read_version(file: &Path, path: &str) -> Result<Version>`
- [x] Implement `write_version(file: &Path, path: &str, version: &Version) -> Result<()>`
- [x] TOML reader/writer that preserves formatting and comments
- [x] JSON reader/writer that preserves formatting
- [x] Plain text reader/writer for VERSION files
- [x] Auto-detect version file if not configured (search for Cargo.toml,
  package.json, etc.)
- [x] Implement `sync_files` execution (e.g., `cargo generate-lockfile` after
  Cargo.toml version change)

Implementation note (2026-03-11):
- Effigy now executes configured `release.sync-files` entries for supported
  Cargo-based release preparation. `Cargo.lock` is currently the supported sync
  target, applied via `cargo check --quiet` after version/changelog mutation and
  surfaced in plan/apply payloads as a sync-file mutation.
- Release version-file writes now preserve existing file layout for
  `Cargo.toml`, `pyproject.toml`, and `package.json`: TOML updates use a
  format-preserving document editor so comments/table ordering survive, while
  JSON updates replace only the targeted string token so spacing and unrelated
  object layout remain intact.

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
- [x] Load release config from `effigy.toml`
- [x] Read current version from version file
- [x] Analyze changelog for unreleased entries (uses `crates/changelog`)
- [x] Compute suggested version bump
- [x] Validate changelog format
- [x] Optional `--check-gates` flag to run gate commands
- [x] JSON output with `--format=json` for scripting
- [x] Exit code: 0 if ready, 1 if blockers exist

Implementation note (2026-03-10):
- This phase currently ships `effigy release status` through Effigy's main
  command surface with `[release]` manifest config support in `src/runner/*`.
- A follow-up batch now also ships non-destructive `effigy release prepare --plan`
  with planned version/changelog mutation previews and optional gate checks.
- The roadmap still uses earlier placeholder paths (`src/config/release.rs`,
  `src/release/status.rs`) as design intent; the shipped code lives in the
  runner command layer and can be refactored later without changing the user
  contract.

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
- [x] Implement step 1: changelog validation (reuse from `crates/changelog`)
- [x] Implement step 2: version proposal with interactive approval
- [x] Implement step 3: changelog formatting with diff display
- [x] Implement step 4: version file updates with preview
- [x] Implement step 5: gate execution with sequential runs and reporting
- [x] Implement step 6: summary and final approval
- [x] Implement `.release-prepared.json` state file writing
- [x] Implement `--dry-run` flag that shows what would happen without prompts
- [x] Handle re-running after partial preparation (detect existing state)
- [x] Support non-interactive mode for CI (`--yes` flag with all approvals
  pre-given — still requires state file for execute)

Implementation note (2026-03-11):
- Effigy now ships `effigy release prepare --plan` as the first non-destructive
  prepare slice. It validates changelog state, derives the planned version,
  previews version-file and changelog mutations, and can include gate results.
- Effigy also now ships a constrained `effigy release prepare --yes` path that
  applies supported version/changelog changes and writes `.release-prepared.json`
  when the plan is valid. This still stops before commit/tag/push.
- Effigy now also ships plain `effigy release prepare` in text mode. It renders
  the prepare preview, prompts for confirmation, and then applies the prepare
  step. When `[release.gates]` is configured, the interactive path automatically
  runs the configured gates rather than requiring a separate `--check-gates`
  opt-in.
- Effigy now also ships staged interactive prepare review: version proposal,
  per-file mutation previews with before/after snippets, gate-result review,
  and a final approval prompt before writing `.release-prepared.json`.
- Re-running the non-interactive prepare path now fails fast when an existing
  `.release-prepared.json` state file is already present, so partial
  preparation is detected instead of silently overwritten.
- Interactive prepare now also supports deliberate custom version override:
  operators can accept the suggested version or enter a different valid semver,
  and the chosen version is carried through mutation previews, prepared output,
  and `.release-prepared.json`.
- Non-interactive prepare now matches that contract with `--version <SEMVER>`
  on `effigy release prepare --plan` and `effigy release prepare --yes`, so
  scripted/operator-approved flows can preview or apply a deliberate override
  while still surfacing suggested-versus-selected version metadata.
- `effigy release simulate` now surfaces the same suggested-versus-selected
  version metadata as prepare, even when no override is active yet, so future
  preview flows do not need a separate metadata model.
- `effigy release simulate --version <SEMVER>` is now available, so the full
  dry-run preview can exercise the same deliberate selected-version contract as
  non-interactive prepare without writing files or state.
- `effigy release prepare --dry-run` and `effigy release execute --dry-run`
  now alias the existing plan/preflight preview surfaces, so the shipped
  command contract supports both `--plan` and the roadmap's original
  `--dry-run` spelling without introducing another result mode.
- `effigy release simulate` and `effigy release prepare --plan` now provide
  richer per-file preview data, including concise inline diff snippets and
  mutation details for supported write mutations, so operator review has more
  signal than the original one-line before/after summaries.
- Plain interactive `effigy release prepare` now supports mutation drill-down
  during Step 2 review: operators can inspect a single planned file mutation in
  detail before accepting or cancelling the prepare flow.
- Plain interactive `effigy release execute` now supports similar drill-down
  inspection for stale-state warnings and working-tree items, and blocked
  execute preflights expose inspectable stale/working-tree issues before
  returning failure.
- Interactive `effigy release prepare` and `effigy release execute` now expose
  compact review menus, so operators can jump between the relevant review
  sections instead of stepping through a fixed linear prompt order every time.
- Those interactive review menus now also keep a compact command legend plus
  current selected-version or stale-acknowledgement summary visible in the menu
  itself, so operators do not need to re-read the prompt footer to see what is
  currently selected or what commands are available.
- Those menus now also mark which review sections were already inspected, and
  blocked prepare/execute output appends suggested remediation actions so
  operators see the likely next fix path instead of only raw blocker strings.
- `effigy release resume` now provides a dedicated prepared-state recovery
  surface: it summarizes `.release-prepared.json`, highlights stale or
  working-tree drift since prepare time, and can hand operators directly back
  into execute review from a recovery menu.
- `.release-prepared.json` now also records source fingerprints for the
  prepared branch, prepared HEAD, and each prepared file digest, and
  `release resume` / `release execute` use them to detect semantic drift such
  as branch movement, HEAD movement, and changed prepared-file contents after
  prepare time.
- Interactive `release resume` and `release execute` now also expose direct
  operator recovery shortcuts for that drift: `gates`, `reprepare`, and
  `discard` are available from the recovery/review menus, and blocked execute
  preflight review also offers those shortcuts before returning failure.
- The remaining prepare UX gap is richer edit-in-place prompting or inline diff
  presentation, not the lack of a deliberate version override path.

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
  → Require explicit acknowledgement/override before continuing

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
- [x] Implement state file loading and validation
- [x] Implement staleness check with the default 1 hour warning threshold
- [x] Implement working tree verification
- [x] Implement final approval prompt
- [x] Implement git commit with conventional message format
- [x] Implement git tag creation
- [x] Implement git push with tag
- [x] Implement state file cleanup
- [x] Print post-release monitoring instructions
- [x] Handle failures gracefully: if push fails, do NOT re-tag

Implementation note (2026-03-11):
- Effigy now ships `effigy release execute --plan` as the first execute slice.
  It loads `.release-prepared.json`, validates the prepared version/tag state,
  warns when the prepared state is older than the default one-hour threshold,
  and verifies the git working tree contains exactly the prepared file changes
  plus the state file.
- Effigy now also ships `effigy release execute --yes` as the first irreversible
  execute path. It requires an explicit `--yes`, creates the release commit,
  creates the prepared tag, pushes branch and tag to `origin`, prints
  post-release monitoring instructions, and removes `.release-prepared.json`
  only after the full execute flow succeeds.
- Effigy now also ships plain `effigy release execute` in text mode. It renders
  the execute preflight, prompts for confirmation, and then runs the same
  commit/tag/push path when approved.
- If push fails, Effigy leaves the prepared state file in place, keeps the
  local commit/tag side effects visible in the result payload, and blocks any
  retry that would attempt to create the same tag again.
- The threshold is currently fixed at the default warning window; making it
  configurable can be treated as a later refinement instead of a blocker for
  the execute preflight contract.
- Effigy now also ships staged interactive execute review: prepared-state
  review, working-tree review, and a final approval prompt before the
  irreversible commit/tag/push path runs.
- Stale prepared state now requires deliberate acknowledgement: text-mode
  execute inserts a stale-state acknowledgement step, while `--plan` and
  `--yes` require explicit `--allow-stale` to proceed past the stale warning.
- The prepared state now also records source fingerprints, and execute/resume
  use them to detect semantic drift beyond raw working-tree presence checks:
  branch drift, HEAD movement, and prepared-file content drift now surface as
  first-class preflight blockers.
- The remaining execute UX gap is deeper operator editing or retry branching,
  not the lack of staged human approvals.

## 6) Release Simulate Command

Implement `effigy release simulate` — full dry-run showing what would happen.

Location: `src/release/simulate.rs`

This is the safe preview mode. It runs the full prepare workflow in read-only
mode, showing every step without modifying any files.

Tasks:
- [x] Run changelog analysis without modifying files
- [x] Show proposed version bump
- [x] Show what formatting changes would be applied
- [x] Show which files would be modified
- [x] Run gates (these are read-only by nature)
- [x] Show the commit message and tag that would be created
- [x] No state file written
- [x] JSON output support for scripting

Implementation note (2026-03-11):
- Effigy now ships `effigy release simulate` as the full dry-run preview. It
  reuses the release context and mutation planning logic, runs configured gates
  with the same fail-fast timing contract as `release gates`, shows the planned
  version/tag/commit message plus mutation previews, and explicitly reports
  that `.release-prepared.json` is not written.
- The shipped implementation lives in `src/runner/release_command.rs` rather
  than the original sketched `src/release/simulate.rs` path, but it satisfies
  the section-6 operator contract without creating a parallel release module.

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
- [x] Load gate definitions from `[release.gates]` config
- [x] Execute gates sequentially with output capture
- [x] Report pass/fail with timing for each gate
- [x] Stop on first failure (fail-fast)
- [x] Show captured output for failed gates
- [x] Support `effigy release gates` standalone command (run gates without
  full prepare workflow)
- [x] Exit code: 0 if all pass, 1 if any fail

Implementation note (2026-03-11):
- Effigy now ships `effigy release gates` as the standalone gate runner. It
  loads `[release.gates]`, executes gates sequentially, records per-gate and
  total timings, stops on the first failure, and surfaces captured stdout/stderr
  for failed gates in both text and JSON output.
- The underlying release gate execution used by `status` and `prepare` now
  shares the same sequential timed runner, so gate metadata stays consistent
  across standalone and integrated release flows.

## 8) Effigy Self-Hosting Migration

Migrate Effigy's own release process onto the new system.

This is the critical validation step. Effigy's release flow now runs through
the built-in `effigy release` commands directly rather than wrapper scripts.

### Native mapping:

| Current | New |
|---------|-----|
| prepare workflow | `effigy release prepare` |
| release-gate validation | `effigy release gates` (+ optional `effigy release verify-install` when `--tag` is provided) |
| Manual `git tag` + `git push` | `effigy release execute` |
| `sed` extraction in release-binaries.yml | `effigy changelog extract` |

### Migration tasks:

- [x] Add `[release]` section to Effigy's `effigy.toml`
- [x] Configure gates to match the real self-hosted release checks
- [x] Configure version file as `Cargo.toml` with `Cargo.lock` sync
- [x] Run migration validation proving the built-in commands preserve the
  intended release behavior
- [x] Update `release-binaries.yml` to use `effigy changelog extract` for
  release notes (requires human approval per CLAUDE.md)
- [x] Update guide 049 (Release Protocol) section 6c to reference new commands
- [x] Update guide 014 (Release Checklist) to reference new workflow
- [x] Retire the legacy release wrapper scripts after the explicit
  wrapper-retirement criteria in guide `049` are met
- [x] Execute first release using `effigy release prepare` + `effigy release
  execute`

Implementation note (2026-03-11):
- Effigy’s root `effigy.toml` now declares `[release]` with the baseline gate
  set mirrored from the real self-hosted release checks, and local self-hosted
  release-gate validation now runs through `effigy release gates`
  (with `cargo qa-release` as the cargo alias).
- `Cargo.lock` sync during prepare is now shipped for Cargo-based repos, with a
  fixture-level migration proof against the earlier wrapper-era prepare path.
- Effigy now also ships `effigy release verify-install` for the tag-install
  validation path.
- The wrapper-era migration proof is now covered in tests and logs, but the
  wrappers themselves have since been retired from the live repo.
- The remaining self-hosting work is now about workflow adoption and successful
  operator usage, not uncertainty about the shipped command mappings.
- Wrapper-retirement is no longer pending in the live repo; the release
  checklist template (`014`) and repo-level operator guidance now point
  maintainers directly at `effigy release simulate/status/prepare/execute`.
- Release-note authoring guidance now explicitly uses `effigy changelog extract
  CHANGELOG.md --version X.Y.Z` as the pre-cutover baseline for drafting notes,
  so the remaining workflow edit is just an approved automation swap rather than
  a new release-notes design decision.

## 9) Cross-Project Adoption Support

Ensure the release system works for projects beyond Effigy.

### Design for variety:

The system must handle:
- **Rust projects** (Cargo.toml, Cargo.lock sync, `cargo test` gates)
- **Node.js projects** (package.json, package-lock.json sync, `npm test` gates)
- **Python projects** (pyproject.toml, `pytest` gates)
- **Multi-language monorepos** (version in VERSION file, heterogeneous gates)

### Validation tasks:

- [x] Test configuration with a Node.js project version file (package.json)
- [x] Test configuration with a plain VERSION file
- [x] Verify gate commands work with non-Rust toolchains
- [x] Document configuration examples for each project type
- [x] Add to Effigy's agent adoption guide (047) with release orchestration
  section

Implementation note (2026-03-11):
- End-to-end CLI coverage now proves release status/prepare behavior across
  `package.json`, `pyproject.toml`, and plain `VERSION` repos, including
  shell-based gate commands that do not rely on Rust toolchains.
- Guide `047` now includes release-orchestration examples for Node.js, Python,
  and multi-language/plain-version repos so agent-facing adoption guidance
  matches the shipped version-file support.

## 10) Documentation and Guides

Update Effigy documentation for the new release orchestration feature.

Tasks:
- [x] Create new guide: `docs/guides/051-release-orchestration.md`
  - Configuration reference
  - Workflow walkthrough
  - Gate configuration
  - Version file formats
  - Migration from custom scripts
- [x] Update guide 049 (CI Binary Distribution and Release Protocol):
  - Section 6c: reference `effigy release` commands
  - Section 7a: update agent protocols for new commands
- [x] Update guide 014 (Release Checklist): integrate with new workflow
- [x] Update CLAUDE.md: reference `effigy release` in Release Protocol section
- [x] Add `effigy release --help` with comprehensive usage text

Implementation note (2026-03-11):
- Guide `051` now serves as the dedicated release-orchestration reference for
  shipped release commands, `[release]` config, version-file formats, gate
  behavior, and migration guidance.
- The docs hub, command matrix, and `CLAUDE.md` now point maintainers and
  agents at that guide instead of requiring them to reconstruct the release
  surface from roadmap notes and batch logs.
- `effigy release --help` was already shipped earlier in `027`; this section
  now records that reality instead of leaving the checklist stale.

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

Status: complete

Closeout note:

- `docs/logs/archive/2026-03/12-131500-release-checkpoint-v0-2-5.md` records the first
  real production Effigy release through the built-in release path.
- `docs/logs/archive/2026-03/11-183500-release-workflow-cutover-hosted-validation.md`
  records the hosted validation for `.github/workflows/release-binaries.yml`
  using `effigy changelog extract`.
- `docs/logs/archive/2026-04/15-013500-release-wrapper-retirement-and-native-cutover.md`
  records the later retirement of the legacy release compatibility wrappers.
- The remaining release work is no longer roadmap `027` implementation work. It
  is normal operator use of the shipped release surface once `g02.010` is fully
  out of the way.

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
- Current release protocol: `docs/guides/049-ci-binary-distribution-and-release-protocol.md`
