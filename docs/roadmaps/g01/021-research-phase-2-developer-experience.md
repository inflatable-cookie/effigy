# 021 - Research Phase 2: Developer Experience

Generation: `g01`

Status: Planned
Owner: Research
Created: 2026-03-07
Depends on: 020

## Vision Alignment

This roadmap extends Effigy research into developer experience concerns: how tools present themselves to users, handle errors, integrate with editors, manage cross-platform complexity, and handle environment configuration. These concerns separate adequate tools from delightful ones.

## Primary Tags

- `RESEARCH`
- `UX`
- `PORTABILITY`

## Target Envelope

Complete comparative analysis of shell completions, error reporting, monorepo workspace patterns, cross-platform handling, and environment management. Build on Phase 1 foundations to create a comprehensive DX pattern library.

## Vision Target Delta

Move from functional CLI to delightful developer experience through systematic study of DX patterns in successful tools.

## 1) Problem

Effigy has functional shell completions, error messages, and cross-platform support, but lacks systematic research into:

- What makes shell completions actually useful vs. just present?
- How should errors be structured for maximum clarity and actionability?
- What workspace patterns do developers expect in monorepos?
- Where do cross-platform assumptions break down?
- How should environment and secrets be handled?

Without research:
- Completions may miss common usage patterns
- Error messages may be technically correct but unhelpful
- Monorepo workflows may feel foreign to users coming from npm/Rush/Nx
- Cross-platform bugs may be discovered late
- Environment handling may be inconsistent or insecure

## 2) Goals

- [ ] Study 5+ shell completion implementations and UX patterns
- [ ] Analyze error reporting in Rustc, ESLint, TypeScript, and others
- [ ] Document monorepo workspace patterns across npm, Rush, Nx, pnpm
- [ ] Catalog cross-platform portability wins and failures
- [ ] Research environment and secret management patterns
- [ ] Create 5 value track syntheses
- [ ] Produce 3-5 translation memos

## 3) Non-Goals

- [ ] No reimplementation of completions during research
- [ ] No redesign of error format during research
- [ ] No new workspace features during research
- [ ] No platform-specific workarounds during research
- [ ] No new env/secret features during research

## 4) Research Tracks

### Track 06: Shell Completions

Key questions:
- Static vs. dynamic completions tradeoffs
- How to handle task name completion
- Context-aware completion (flags, paths, task names)
- Installation and update mechanisms

Tools to study:
- cargo (task-aware, dynamic)
- npm (basic, static)
- pnpm (rich completions)
- ripgrep (excellent static)
- fd (simple but effective)
- git (the gold standard)

Deliverables:
- Tool dossiers for cargo, git, ripgrep
- Value track synthesis on completion patterns
- Translation memo on Effigy completion improvements

### Track 07: Error Reporting and Diagnostics

Key questions:
- Error message structure (what happened, why, how to fix)
- Suggestion systems and fix-it hints
- Error codes and documentation linking
- Span/locations and context display

Tools to study:
- Rustc (industry-leading error messages)
- ESLint (rule-based, configurable)
- TypeScript (type error complexity)
- Elm (friendly error philosophy)
- Clippy (lint suggestions)

Deliverables:
- Tool dossiers for Rustc, ESLint, Elm
- Value track synthesis on error patterns
- Translation memo on Effigy error improvements

### Track 08: Monorepo Discovery and Workspaces

Key questions:
- How do tools discover workspace structure?
- What workspace configuration patterns exist?
- How is task scoping handled across workspaces?
- What are the tradeoffs between centralized and decentralized config?

Tools to study:
- npm workspaces (Node.js standard)
- pnpm workspaces (performance-focused)
- Rush (Microsoft's approach)
- Nx (task graph focused)
- Yarn Berry (Plug'n'Play)

Deliverables:
- Tool dossiers for Rush, Nx
- Value track synthesis on workspace patterns
- Translation memo on Effigy workspace validation

### Track 09: Cross-Platform Portability

Key questions:
- Shell choice and compatibility
- Path handling differences
- Process spawning variations
- Terminal/ANSI support across platforms

Tools to study:
- Just (explicit cross-platform focus)
- Task (Go's portability)
- PowerShell (Windows-native)
- Git (ubiquitous but complex)
- Deno (modern cross-platform)

Deliverables:
- Tool dossiers for Just, Deno
- Value track synthesis on portability patterns
- Translation memo on Effigy platform coverage

### Track 10: Environment and Secret Management

Key questions:
- .env file loading patterns and precedence
- Secret injection without exposure
- Environment variable inheritance
- Per-task environment customization

Tools to study:
- direnv (directory-specific env)
- dotenv (standard .env loading)
- 1Password CLI (secret injection)
- op CLI (1Password's new CLI)
- Vault (enterprise secrets)

Deliverables:
- Tool dossiers for direnv, 1Password CLI
- Value track synthesis on environment patterns
- Translation memo on Effigy env/security review

## 5) Execution Plan

### Batch 21.1 - Track 06: Shell Completions ✅ COMPLETE

- [x] Create git dossier (completion gold standard)
- [x] Create ripgrep dossier (static completions with clap_complete)
- [x] Synthesize Track 06 value track
- [x] Draft Translation Memo 006: Completion UX

**Outcome**: Hybrid approach validated. Use clap_complete for static flags + dynamic runtime completion for task names.

### Batch 21.2 - Track 07: Error Reporting ✅ COMPLETE

- [x] Create Rustc dossier (error messages)
- [x] Create ESLint dossier (rule-based errors)
- [x] Synthesize Track 07 value track
- [x] Draft Translation Memo 007: Error Messages

**Outcome**: Rustc-inspired error format recommended. Key: clear messages, precise locations, suggestions, error codes, JSON output.

### Batch 21.3 - Track 08: Monorepo Workspaces ✅ COMPLETE

- [x] Create Rush dossier (workspace management)
- [x] Create Nx dossier (task graphs)
- [x] Synthesize Track 08 value track
- [x] Draft Translation Memo 008: Workspace Patterns

**Outcome**: Keep distributed catalogs, add: change detection, visualization, cross-catalog dependencies.

### Batch 21.4 - Track 09: Cross-Platform ✅ COMPLETE

- [x] Create Just dossier (cross-platform focus)
- [x] Create Deno dossier (modern portability)
- [x] Synthesize Track 09 value track
- [x] Draft Translation Memo 009: Portability

**Outcome**: Keep native shell approach. Add platform conditionals and path helpers. Ensure Windows CI testing.

### Batch 21.5 - Track 10: Environment Management ✅ COMPLETE

- [x] Create direnv dossier (directory-specific environment)
- [x] Create 1Password CLI dossier (secret injection)
- [x] Synthesize Track 10 value track
- [x] Draft Translation Memo 010: Environment & Secrets

**Outcome**: Keep TOML env section, add .env file loading and secret provider integration. Clear precedence rules.

### Batch 21.6 - Synthesis and Promotion

- [ ] Update research README with Phase 2 findings
- [ ] Promote stable conclusions to `docs/concepts/`
- [ ] Create DX pattern library document
- [ ] Identify implementation tickets for roadmap g01.022

## 6) Acceptance Criteria

- [ ] 10+ tool dossiers complete (cumulative from Phase 1)
- [ ] 10 value track syntheses complete (cumulative)
- [ ] 10 translation memos complete (cumulative)
- [ ] At least 2 Phase 2 memos promoted to `docs/concepts/`
- [ ] DX pattern library document created
- [ ] Research backlog identified for Phase 3

## 7) Risks and Mitigations

- [ ] Risk: Phase 1 findings invalidate Phase 2 approaches
  - Mitigation: Build on Phase 1, revisit only if major contradictions found
- [ ] Risk: DX is subjective, hard to synthesize objectively
  - Mitigation: Focus on observable patterns, not aesthetic preferences
- [ ] Risk: Tool examples are language-specific (Rust, JS)
  - Mitigation: Explicitly generalize patterns across languages

## 8) Deliverables

- [ ] Tool dossiers (10+ cumulative)
- [ ] Value track syntheses (10 cumulative)
- [ ] Translation memos (10 cumulative)
- [ ] DX pattern library document
- [ ] Promoted concepts

## 9) Validation

- [ ] Each dossier follows template
- [ ] Each value track has cross-tool comparison table
- [ ] Each translation memo has explicit recommendation
- [ ] Pattern library is referenceable from implementation work

## 10) Outcome

Status: planned

Upon completion, Effigy will have:
- Validated completion patterns
- Structured approach to error messages
- Workspace design informed by monorepo tools
- Cross-platform coverage validated
- Environment/security patterns documented

Next: Research Phase 3 (Scale & Integration) roadmap g01.022

