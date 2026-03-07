# Translation Memo 012: CI/CD Integration

**Status:** Draft  
**Track:** 12 - CI/CD Integration  
**Tools:** GitHub Actions, pre-commit  
**Date:** 2026-03-07  
**Related:** Translation Memo 011 (Remote Execution), Translation Memo 010 (Environment)

## Executive Summary

This memo translates Track 12 research findings into concrete implementation guidance for Effigy's CI/CD integration strategy. The key insight: **Effigy should be CI-agnostic, providing a unified interface that works consistently across local development, git hooks, and any CI provider.**

## Research Summary

### GitHub Actions
- **Strengths**: Native GitHub integration, generous free tier, huge marketplace, matrix builds
- **Weaknesses**: YAML complexity, vendor lock-in, debugging difficulty, startup overhead
- **Pattern**: Event-driven workflows with reusable actions

### pre-commit
- **Strengths**: Multi-language support, configuration-driven, caching, selective execution
- **Weaknesses**: Python dependency, hook startup overhead, configuration complexity
- **Pattern**: Git hook framework with ecosystem of hooks

### Common Pattern
Both tools suffer from configuration drift between local and CI environments. The solution is a single source of truth (Effigy configuration) that drives both.

## Core Principles

### 1. CI-Agnostic Design

Effigy should work with any CI provider (GitHub Actions, GitLab CI, CircleCI, etc.) through the same interface:

```bash
effigy ci        # Full CI suite
effigy ci --fast # Fast checks (local/PR)
```

### 2. Unified Configuration

One `effigy.toml` drives everything:
- Local development
- Git hooks
- CI execution
- Matrix builds

### 3. Layered Automation

| Layer | Speed | Purpose | Implementation |
|-------|-------|---------|----------------|
| Editor | Instant | Syntax, formatting | LSP, formatters |
| Git Hooks | < 10s | Fast checks | `effigy ci --fast` |
| Pre-push | < 60s | Local validation | `effigy test` |
| CI | Minutes | Full validation | `effigy ci` |

## Proposed Implementation

### Phase 1: Core CI Command

**`effigy ci` command:**

```bash
# Basic usage
effigy ci              # Run full CI suite (defined in effigy.toml)
effigy ci --fast       # Fast checks only (for git hooks)
effigy ci --matrix     # Simulate matrix locally

# Output formats
effigy ci --format default    # Human readable
effigy ci --format github     # GitHub Actions annotations
effigy ci --format junit      # JUnit XML for CI reporting
```

**Configuration:**

```toml
# effigy.toml
[ci]
# Tasks to run in CI
tasks = ["lint", "test", "build", "check"]

# Fast mode (for git hooks)
fast-tasks = ["lint", "test --fast"]

# Environment variables set in CI mode
[ci.env]
CI = "true"
RUST_BACKTRACE = "1"

# Caching configuration (used by CI providers)
[ci.cache]
paths = ["target/", ".cargo/registry/", ".effigy/cache/"]
key = "${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}"
```

### Phase 2: Git Hooks

**Hook configuration:**

```toml
# effigy.toml
[hooks]
pre-commit = ["effigy ci --fast"]
pre-push = ["effigy test"]
post-checkout = ["effigy cache gc"]
post-merge = ["effigy install"]

[hooks.settings]
install-on-init = true  # Auto-install hooks on first run
skip-on-ci = true       # Skip hooks when CI=true
```

**Commands:**

```bash
effigy hooks install          # Install git hooks
effigy hooks uninstall        # Remove git hooks
effigy hooks run pre-commit   # Run specific hook manually
effigy hooks list             # Show configured hooks
```

### Phase 3: CI Configuration Generation

**Generate CI configs:**

```bash
# GitHub Actions
effigy ci init --provider github-actions

# Generates .github/workflows/ci.yml:
# name: CI
# on: [push, pull_request]
# jobs:
#   test:
#     runs-on: ubuntu-latest
#     steps:
#       - uses: actions/checkout@v4
#       - run: curl -sSL https://effigy.dev/install.sh | sh
#       - run: effigy ci
```

```bash
# GitLab CI
effigy ci init --provider gitlab-ci

# Generates .gitlab-ci.yml:
# stages: [test]
# test:
#   script:
#     - curl -sSL https://effigy.dev/install.sh | sh
#     - effigy ci
```

### Phase 4: Matrix Testing

**Local matrix simulation:**

```bash
effigy ci --matrix --platforms linux,macos,windows
```

Uses containers (Docker/Podman) to simulate different platforms locally.

## pre-commit Integration

### Option A: Native Hooks (Recommended)

Replace pre-commit with Effigy's native hook management:

```toml
[hooks]
pre-commit = ["effigy ci --fast"]
```

Benefits:
- No Python dependency
- Single configuration
- Better performance

### Option B: pre-commit Compatibility

For teams already using pre-commit:

```yaml
# .pre-commit-config.yaml
repos:
  - repo: local
    hooks:
      - id: effigy
        name: Effigy
        entry: effigy ci --fast
        language: system
        pass_filenames: false
```

This allows gradual migration.

## Implementation Priorities

| Priority | Feature | Rationale |
|----------|---------|-----------|
| P0 | `effigy ci` command | Core CI functionality |
| P0 | Git hooks management | Local quality gates |
| P1 | GitHub Actions example | Most common CI |
| P1 | Output formats (github, junit) | CI integration |
| P2 | CI config generation | Developer convenience |
| P2 | pre-commit compatibility | Migration path |
| P3 | Local matrix simulation | Advanced feature |
| P3 | GitLab CI/CircleCI examples | Multi-provider |

## Open Questions

1. Should Effigy provide an official GitHub Action wrapper?
2. How to handle CI secrets vs local environment variables?
3. Should hooks be defined per-catalog in monorepos?
4. How to integrate with remote execution (Track 11)?

## Success Criteria

- Single `effigy ci` command works locally and in CI
- Git hooks install and run in < 10 seconds
- Configuration is DRY (no duplication between local and CI)
- Works with GitHub Actions, GitLab CI, and CircleCI
- Provides clear migration path from pre-commit

## Related Concepts

- Concept: Git Hooks Management
- Concept: CI-Agnostic Execution
- Concept: Output Format Adapters
- Roadmap: Phase 3, Track 12

