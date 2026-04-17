# 020 - Research Phase 1: Core Execution

Generation: `g01`

Status: Complete
Owner: Research
Created: 2026-03-07
Depends on: Research skeleton establishment

## Vision Alignment

This roadmap establishes the foundational research tracks for Effigy's core execution model. By studying task configuration formats, caching strategies, watch modes, DAG execution, and process management patterns across established tools, Effigy can validate its architectural choices and identify proven patterns to adopt, pitfalls to avoid, and gaps to differentiate.

## Primary Tags

- `RESEARCH`
- `ARCH`
- `UX`

## Target Envelope

Complete comparative analysis of task runners and build systems covering configuration formats, caching, file watching, dependency scheduling, and process management. Produce tool dossiers, value track syntheses, and translation memos that inform Effigy design decisions.

## Vision Target Delta

Move from intuitive feature development to evidence-based design grounded in cross-tool comparative analysis.

## 1) Problem

Effigy has built a working task runner with TOML configuration, DAG execution, caching, watch mode, and TUI process management. However, many design decisions were made based on intuition rather than systematic study of existing tools:

- Why TOML? Is it better than YAML or custom syntax for this use case?
- How should caching work? What can be learned from Bazel, Turbo, sccache?
- Is Effigy's watch mode implementation aligned with best practices?
- Does the DAG scheduler handle edge cases that other tools have solved?
- Is the TUI process manager solving problems that cargo, pnpm, or Bazel have already addressed?

Without comparative research, Effigy risks:
- Reinventing wheels poorly
- Missing established UX patterns users expect
- Over-engineering where simple solutions suffice
- Under-engineering where complexity is justified

## 2) Goals

- [ ] Create comprehensive dossiers for 5+ task runners (Make, Just, Task, Bazel, cargo)
- [ ] Synthesize 5 value tracks covering core execution concerns
- [ ] Produce 3-5 translation memos with actionable recommendations
- [ ] Validate or revise Effigy's TOML configuration choice
- [ ] Identify caching improvements from build system research
- [ ] Benchmark watch mode patterns against established tools
- [ ] Document DAG execution edge cases and solutions
- [ ] Catalog TUI/process management UX patterns

## 3) Non-Goals

- [ ] No implementation changes during research (only documentation)
- [ ] No competitive feature parity analysis (focus on patterns, not checklists)
- [ ] No benchmarking of other tools' performance (focus on design patterns)
- [ ] No user surveys (focus on tool analysis, not market research)
- [ ] No patent or license analysis (assumes OSS tools)

## 4) Research Tracks

### Track 01: Task Configuration and Manifest Formats

Key questions:
- What configuration formats do successful tools use?
- What are the tradeoffs between custom syntax and structured data?
- How much expressiveness is needed vs. simplicity?

Tools to study:
- Make (custom syntax, tab-sensitive)
- Just (custom syntax, modern)
- Task (YAML, structured)
- npm (JSON, limited)
- mise/rtx (TOML, similar to Effigy)

Deliverables:
- Tool dossiers for Make, Just, Task
- Value track synthesis comparing formats
- Translation memo on TOML validation

### Track 02: Caching Strategies

Key questions:
- Input/output hashing vs. timestamp-based caching
- Local vs. remote caching tradeoffs
- Cache invalidation strategies
- Cache storage backends

Tools to study:
- Bazel (content-addressable, remote caching)
- Turbo (incremental, remote)
- Nx (computation caching)
- sccache (compiler caching)
- Pants (fine-grained)

Deliverables:
- Tool dossiers for Bazel, Turbo, sccache
- Value track synthesis on caching patterns
- Translation memo on Effigy caching improvements

### Track 03: Watch Mode and File Monitoring

Key questions:
- Polling vs. OS-specific notify mechanisms
- Debouncing and event coalescing
- Cross-platform consistency
- Resource usage at scale

Tools to study:
- cargo-watch (Rust ecosystem standard)
- watchexec (general-purpose)
- entr (Unix philosophy)
- Turbo (incremental builds)
- Vite (fast HMR)

Deliverables:
- Tool dossiers for cargo-watch, watchexec
- Value track synthesis on file watching
- Translation memo on Effigy watch improvements

### Track 04: DAG Execution and Dependency Scheduling

Key questions:
- How to represent task dependencies
- Parallel execution strategies
- Cycle detection and error handling
- Incremental execution with partial failures

Tools to study:
- Make (file-based DAG)
- Bazel (Skyframe evaluation)
- Dagger (container-based DAG)
- Airflow (orchestration patterns)
- Prefect/Dagster (modern orchestration)

Deliverables:
- Tool dossiers for Bazel, Dagger
- Value track synthesis on DAG patterns
- Translation memo on scheduler validation

### Track 05: Process Management and TUI Patterns

Key questions:
- How to display concurrent process output
- ANSI handling and terminal emulation
- Keyboard interaction patterns
- Progress indication for long-running tasks

Tools to study:
- cargo (Rust build output)
- pnpm (concurrent package scripts)
- Bazel (build event stream)
- yarn (classic vs. modern)
- Docker Compose (multi-service output)

Deliverables:
- Tool dossiers for cargo, pnpm
- Value track synthesis on TUI patterns
- Translation memo on Effigy TUI improvements

## 5) Tool Dossier Template

Each dossier should follow the template in `docs/research/templates/tool-dossier-template.md`:

1. Why this tool matters
2. Product and era context
3. Defining architectural bets
4. Standout strengths
5. Chronic weaknesses
6. Between-release corrections
7. Effigy-relevant lessons
8. Source inventory
9. Open questions

## 6) Execution Plan

### Batch 20.1 - Track 01: Configuration Formats ✅ COMPLETE

- [x] Complete Make dossier (baseline)
- [x] Complete Just dossier (modern syntax)
- [x] Complete Task dossier (YAML approach)
- [x] Synthesize Track 01 value track
- [x] Draft Translation Memo 001: TOML Configuration Validation

**Outcome**: TOML validated as correct choice. See log `2026-03/07-200000-research-batch-20-1-track-01-completion.md`.

### Batch 20.2 - Track 02: Caching Strategies ✅ COMPLETE

- [x] Create Bazel dossier (content-addressable caching)
- [x] Create Turbo dossier (incremental caching)
- [x] Create sccache dossier (compiler caching)
- [x] Synthesize Track 02 value track
- [x] Draft Translation Memo 002: Caching Strategy

**Outcome**: Content-addressable caching validated. Proposed design: local + remote tiers, configurable per-task inputs, HTTP-based protocol. See log for details.

### Batch 20.3 - Track 03: Watch Mode ✅ COMPLETE

- [x] Create cargo-watch dossier (Cargo-native watcher)
- [x] Create watchexec dossier (cross-platform library)
- [x] Create entr dossier (Unix philosophy minimalism)
- [x] Synthesize Track 03 value track
- [x] Draft Translation Memo 003: File Watching

**Outcome**: watchexec crate recommended for Effigy integration. Key findings: debouncing essential, smart defaults reduce noise, cross-platform abstraction necessary.

### Batch 20.4 - Track 04: DAG Execution ✅ COMPLETE

- [x] Deepen Bazel dossier (Skyframe evaluation model)
- [x] Create Dagger dossier (container-based DAG)
- [x] Synthesize Track 04 value track
- [x] Draft Translation Memo 004: Dependency Scheduling

**Outcome**: Current DAG model validated. Recommend adding: cycle detection, task graph visualization. See log for details.

### Batch 20.5 - Track 05: Process Management ✅ COMPLETE

- [x] Create cargo dossier (output handling, progress indication)
- [x] Create pnpm dossier (concurrent output, workspace patterns)
- [x] Synthesize Track 05 value track
- [x] Draft Translation Memo 005: TUI Patterns

**Outcome**: TUI approach validated. Recommend: help overlay, output prefixing option, continued ANSI improvements. See log for details.

### Batch 20.6 - Synthesis and Promotion

- [ ] Update research README with findings
- [ ] Promote stable conclusions to `docs/concepts/`
- [ ] Identify implementation tickets for roadmap g01.021
- [ ] Create gap analysis document

## 7) Acceptance Criteria

- [ ] 8+ tool dossiers complete with source inventories
- [ ] 5 value track syntheses with cross-tool comparisons
- [ ] 5 translation memos with actionable recommendations
- [ ] Source map updated with all referenced sources
- [ ] At least 2 memos promoted to `docs/concepts/`
- [ ] Research methods documented for future phases

## 8) Risks and Mitigations

- [ ] Risk: Research expands beyond scope ("just one more tool")
  - Mitigation: Strict batch boundaries, time-box each dossier to 2-4 hours
- [ ] Risk: Analysis paralysis, no actionable conclusions
  - Mitigation: Translation memos required to have explicit recommendations
- [ ] Risk: Findings become outdated as tools release new versions
  - Mitigation: Version-pin dossiers, note date of analysis
- [ ] Risk: Research doesn't translate to implementation priority
  - Mitigation: Direct link from memos to roadmap tickets

## 9) Deliverables

- [ ] Tool dossiers (8+ files in `docs/research/tool-dossiers/`)
- [ ] Value track syntheses (5 files in `docs/research/value-tracks/`)
- [ ] Translation memos (5 files in `docs/research/translation-memos/`)
- [ ] Updated source maps
- [ ] Promotion of validated concepts to `docs/concepts/`
- [ ] Gap analysis document

## 10) Validation

- [ ] Each dossier has source inventory with confidence ratings
- [ ] Each value track compares at least 3 tools
- [ ] Each translation memo has explicit recommendation
- [ ] `bash docs/scripts/check-vision-metadata.sh` passes
- [ ] Research README accurately reflects status

## 11) Outcome

Status: complete

The research corpus for Phase 1 is materially complete. The core execution
tracks were researched strongly enough to validate Effigy's direction around
configuration, caching, watch mode, DAG execution, and process/TUI patterns.
The remaining unchecked items in this file were research-program promotion and
index hygiene, not missing core research.

Upon completion, Effigy has:
- Validated or revised TOML configuration choice
- Identified caching improvements from build system research
- Documented watch mode best practices
- Cataloged DAG execution edge cases
- Established TUI pattern library

Next: treat the remaining research-program promotion work as part of the
cross-phase carry-forward in
[`g02.018`](../g02/018-research-promotion-and-carry-forward.md).
