# Comparative Task Runner Research

Purpose: give Effigy a durable place to study existing task runners, build systems, monorepo tools, and developer workflow products without mixing raw research into concept contracts or execution roadmaps.

## Why this exists

Effigy needs to learn from existing tools before implementation pressure forces the same mistakes locally. This area is for comparative analysis, not copying implementations.

Use it to answer:
- which tools or approaches are worth studying for a given problem
- what architectural and UX bets made them successful
- what chronic problems appeared as they scaled
- what changed between major releases to correct those problems
- what Effigy should study further, adapt thoughtfully, or reject early

## Structure

- `master-index.md`: navigate from architecture or delivery concerns to relevant research
- `research-to-implementation-playbook.md`: workflow for using research during implementation
- `quick-start-checklist.md`: short daily checklist for research-aware delivery
- `research-to-architecture-crossref.md`: track where memo findings are aligned, missing, or still prototype-gated
- `gaps-found-during-implementation.md`: capture missing research discovered while building
- `tool-dossiers/`: per-tool specimen files (task runners, build systems, monorepo tools, package managers)
- `value-tracks/`: cross-tool syntheses by problem area (caching, watch modes, DAG execution, etc.)
- `source-hubs/`: curated source maps and source-quality hierarchy
- `translation-memos/`: Effigy-facing recommendations derived from research
- `templates/`: reusable templates for research and implementation traceability
- `discovery-intake.md`: policy for secondary-channel triage and intake rules
- `discovery-triage-log.md`: staging area for signals from secondary channels
- `carry-forward-intake.md`: future-facing research residue that should stay
  out of the active product roadmap queue until re-researched and promoted

## Operating Model

1. **Start with a problem, not a feature wishlist.**
   - What workflow friction exists in current tools?
   - What scaling problems appear in large repos?

2. **Gather primary sources before secondary commentary.**
   - Documentation, source code, release notes first
   - Blog posts, conference talks second
   - Community discussion third

3. **Record strengths, chronic failures, and between-release corrections together.**
   - No tool is all good or all bad
   - Note what they fixed and when

4. **Convert findings into Effigy implications only after cross-tool comparison.**
   - Single-tool study is insufficient for decisions
   - Patterns emerge from comparison

5. **Promote stable conclusions into `docs/architecture/` or `docs/roadmaps/` only when the recommendation is specific enough to constrain design or execution.**

## Source Hierarchy

Prefer sources in this order:

1. **Primary sources**: official docs, release notes, source trees, API references, technical talks, and postmortems
2. **First-party programs**: vendor engineering blogs (when technical), conference presentations with specific claims, SDK documentation
3. **Practitioner analysis**: developer experience reports, migration stories, benchmark studies
4. **Community synthesis**: only when it points back to stronger sources or documents observable behavior

## Research Outputs

Every meaningful research batch should leave at least one durable artifact:
- a tool dossier update
- a value-track synthesis
- a source-hub update
- an Effigy translation memo

Use the templates in [`templates/README.md`](./templates/README.md) so comparisons stay consistent.

## Using This Research During Delivery

When research starts actively shaping implementation work:
1. Check `master-index.md` to find the relevant memo, value track, dossier, and delivery docs.
2. Use `research-to-implementation-playbook.md` for the expected discovery -> decision -> implementation -> review loop.
3. Use `research-to-architecture-crossref.md` to see which memo findings are already represented in architecture, guides, or roadmap work and which are still open.
4. Record missing research in `gaps-found-during-implementation.md` instead of losing it in PR chatter.
5. Use `templates/implementation-decision-record.md` when implementation choices need durable research traceability.

## Promotion Rule

Keep tentative findings here until they can answer all of:
- what problem Effigy is solving
- which evidence supports the recommendation
- which tradeoffs Effigy accepts
- what must be measured or prototyped before adoption

## Current Status

**Phase 1 COMPLETE ✅ | Phase 2 COMPLETE ✅ | Phase 3 COMPLETE ✅ | Track 16 (Additional) COMPLETE ✅**

| Phase | Batch | Track | Status |
|-------|-------|-------|--------|
| 1 | 20.1 | 01: Task Configuration | ✅ Complete |
| 1 | 20.2 | 02: Caching Strategies | ✅ Complete |
| 1 | 20.3 | 03: Watch Mode | ✅ Complete |
| 1 | 20.4 | 04: DAG Execution | ✅ Complete |
| 1 | 20.5 | 05: Process Management | ✅ Complete |
| 2 | 21.1 | 06: Shell Completions | ✅ Complete |
| 2 | 21.2 | 07: Error Reporting | ✅ Complete |
| 2 | 21.3 | 08: Monorepo Workspaces | ✅ Complete |
| 2 | 21.4 | 09: Cross-Platform Portability | ✅ Complete |
| 2 | 21.5 | 10: Environment Management | ✅ Complete |
| 3 | 22.1 | 11: Remote Execution | ✅ Complete |
| 3 | 22.2 | 12: CI/CD Integration | ✅ Complete |
| 3 | 22.3 | 13: IDE Integration | ✅ Complete |
| 3 | 22.4 | 14: Plugin Architecture | ✅ Complete |
| 3 | 22.5 | 15: Telemetry | ✅ Complete |
| — | 23.1 | 16: Secure Secrets Management | ✅ Complete |

**Cumulative Deliverables:**
- 36 tool dossiers (excluding README)
- 16 value track syntheses (Tracks 01-16)
- 16 translation memos (001-016)

## Implementation Bridge Status

The implementation bridge is now bootstrapped:
- `master-index.md` maps architecture and delivery concerns to the current research corpus
- `research-to-implementation-playbook.md` defines the expected workflow for research-aware implementation
- `research-to-architecture-crossref.md` tracks which memo findings are already reflected in docs and code direction
- `gaps-found-during-implementation.md` is ready to capture unanswered questions discovered while building
- `templates/implementation-decision-record.md` provides durable traceability for implementation decisions

**Research Program COMPLETE ✅**

All 15 original tracks + 1 additional track (16: Secure Secrets Management) complete.
Ready for implementation phase informed by research.

**New Track:**
| — | 23.1 | 16: Secure Secrets Management | ✅ Complete | age-based encryption, schema validation

**Candidate tracks for initial research** (see Suggested Tracks below):
- Task definition and configuration formats
- Caching strategies (input/output hashing, remote caching)
- Watch mode and file system monitoring
- DAG execution and dependency resolution
- Process management and TUI patterns
- Shell completion generation
- Monorepo workspace discovery and traversal
- Cross-platform portability concerns

## Suggested Research Tracks

### Phase 1: Core Execution (Tracks 1-5)

| Track | Focus | Key Tools to Study |
|-------|-------|-------------------|
| 1 | Task Configuration & Manifest Formats | Make, Just, Task, npm scripts, Earthly |
| 2 | Caching Strategies | Bazel, Turbo, Nx, Pants, sccache |
| 3 | DAG Execution & Scheduling | Make, Bazel, Dagger, Airflow |
| 4 | Watch Mode & File Monitoring | watchexec, cargo-watch, entr, Turbo |
| 5 | Process Management & TUI | cargo, npm, yarn, pnpm, Bazel |

### Phase 2: Developer Experience (Tracks 6-10)

| Track | Focus | Key Tools to Study |
|-------|-------|-------------------|
| 6 | Shell Completions | cargo, npm, pnpm, ripgrep, fd |
| 7 | Error Reporting & Diagnostics | Rustc, cargo, ESLint, TypeScript |
| 8 | Monorepo Discovery & Workspaces | npm/pnpm workspaces, Rush, Nx, Turborepo |
| 9 | Cross-Platform Portability | Make, Just, Task, PowerShell, Bash |
| 10 | Environment & Secret Management | direnv, dotenv, 1Password CLI, op CLI |

### Phase 3: Scale & Integration (Tracks 11-15)

| Track | Focus | Key Tools to Study |
|-------|-------|-------------------|
| 11 | Remote Execution & Distributed Builds | Bazel, BuildBuddy, Buildkite, GitHub Actions |
| 12 | CI/CD Integration | GitHub Actions, GitLab CI, CircleCI, pre-commit |
| 13 | IDE & Editor Integration | VS Code tasks, JetBrains run configs, LSP |
| 14 | Plugin/Extension Architecture | ESLint, Prettier, Bazel rules, npm scripts |
| 15 | Telemetry & Analytics | cargo (metrics), npm, Homebrew, VS Code |

## Next Task

1. Create initial tool dossiers for Make, Just, and Task (taskfile.dev) to establish the pattern
2. Synthesize Track 1: Task Configuration Formats
3. Write first translation memo on configuration design tradeoffs

---

## Tool Categories to Map

### Task Runners (Direct Competition)
- **Make** — the baseline, ubiquitous but dated
- **Just** — modern syntax, command-runner focus
- **Task** (taskfile.dev) — YAML-based, popular in Go community
- **npm/yarn/pnpm scripts** — Node.js ecosystem default
- **cargo xtask** — Rust ecosystem pattern
- **Invoke** (Python) — Python task runner
- **Fabric** (Python) — SSH-oriented but relevant

### Build Systems (Upstream Inspiration)
- **Bazel** — Google's build system, hermetic builds, remote caching
- **Buck2** — Meta's build system, Rust-based
- **Pants** — Python-focused but multi-language
- **Please** — Thought Machine's build system
- **Ninja** — low-level, fast, used by Chrome, LLVM

### Monorepo Tools (Adjacent Space)
- **Nx** — Angular-focused, expanding to general monorepos
- **Turborepo** — Vercel's tool, caching-focused
- **Rush** — Microsoft's monorepo tool
- **Lage** — Microsoft's task runner
- **moon** — modern task runner for monorepos

### Modern Workflow Tools (Emerging Patterns)
- **Earthly** — container-based builds
- **Dagger** — programmable CI/CD pipelines
- **Wolfi** — container ecosystem
- **act** — local GitHub Actions runner
- **pre-commit** — git hook framework

### Package Managers (Workflow Integration)
- **cargo** — Rust's package manager, excellent CLI UX
- **pnpm** — fast, disk-efficient Node.js package manager
- **Homebrew** — macOS package manager, good CLI patterns
- **nix** — functional package manager, reproducibility focus

---

## Research Principles

1. **Problem-first, not feature-first**
   - Don't ask "what features does X have?"
   - Ask "what workflow problem does X solve and how?"

2. **Evidence over opinion**
   - Link to source code, docs, release notes
   - Note when claims are unverified

3. **Scale-aware analysis**
   - What works for small repos may fail at scale
   - What works at scale may be overkill for small repos

4. **Cross-platform lens**
   - Effigy targets macOS, Linux, Windows
   - Note platform-specific assumptions in other tools

5. **Integration over isolation**
   - How does the tool fit into existing workflows?
   - What are the migration costs?
