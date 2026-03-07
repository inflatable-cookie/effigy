# Track 12: CI/CD Integration

Status: Draft
Value track: CI/CD Integration (GitHub Actions, pre-commit)
Created: 2026-03-07
Tools covered: GitHub Actions, pre-commit, GitLab CI patterns, CircleCI

## 1) Synthesis

### Common Patterns

| Pattern | GitHub Actions | pre-commit | Description |
|---------|---------------|------------|-------------|
| Configuration | YAML workflows | YAML config | Declarative, version controlled |
| Event triggers | Rich (push, PR, schedule) | Git hooks only | Actions more flexible |
| Matrix builds | Native support | N/A | CI-specific capability |
| Reusability | Actions marketplace | Hook repos | Both have ecosystem models |
| Local execution | act (limited) | Full support | pre-commit excels locally |
| Multi-language | Via actions | Native | pre-commit designed for this |
| Caching | Built-in | Virtualenv caching | Both important for performance |

### Key Insights

**CI as an extension of local workflow**

The best CI setup mirrors local development:
```yaml
# .github/workflows/ci.yml
- run: effigy install  # Same as locally
- run: effigy build    # Same as locally
- run: effigy test     # Same as locally
```

Benefits:
- No configuration drift
- Easier debugging
- Reproducible builds

**Two layers of automation**

| Layer | Tool | Purpose | Speed |
|-------|------|---------|-------|
| Local | pre-commit, Effigy hooks | Fast feedback | < 10s |
| CI | GitHub Actions | Full validation | Minutes |

Both needed, not competing.

**Configuration is overhead**

Every tool adds YAML to learn:
- `.github/workflows/*.yml` (GitHub Actions)
- `.pre-commit-config.yaml` (pre-commit)
- `effigy.toml` (Effigy)

Goal: Minimize redundancy while maintaining flexibility.

### What Works

**GitHub Actions patterns:**
- Simple workflows over complex ones
- Composite actions for reuse
- Matrix builds for coverage
- OIDC for secret management

**pre-commit patterns:**
- Fast hooks for local feedback
- Same checks in CI (via `pre-commit run --all-files`)
- Selective execution by file type
- System hooks for existing tools

**Integration patterns:**
- Single source of truth (Effigy.toml)
- CI calls Effigy, not individual tools
- Local and CI use same commands

### What Doesn't

**Anti-patterns:**
- Duplicating tool configs in CI and local
- Complex workflows that are hard to debug
- Slow hooks that developers skip
- GitHub Actions vendor lock-in

**Pain points:**
- YAML debugging
- Local/CI drift
- Hook startup overhead
- GitHub Actions debugging difficulty

## 2) Cross-Tool Capabilities Matrix

| Capability | GitHub Actions | pre-commit | Effigy Should |
|------------|---------------|------------|---------------|
| **Configuration** | YAML workflows | YAML config | TOML (consistent) |
| **Triggers** | Events, schedule | Git hooks | Both (hooks + events) |
| **Matrix builds** | Native | N/A | Delegate to CI |
| **Caching** | Built-in | Virtualenv | Integrate both |
| **Reusability** | Marketplace | Hook repos | Task templates |
| **Local execution** | Limited | Full | Primary focus |
| **Multi-language** | Via actions | Native | First-class |
| **Secret management** | Native | N/A | CI env vars |
| **Status reporting** | Checks API | N/A | CI integration |

## 3) Integration patterns

### Pattern 1: Effigy as the interface

```yaml
# .github/workflows/ci.yml
name: CI
on: [push, pull_request]

jobs:
  test:
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - name: Install Effigy
        run: curl -sSL https://effigy.dev/install.sh | sh
      - name: Run CI
        run: effigy ci  # Single command, consistent everywhere
```

Benefits:
- One configuration (effigy.toml)
- Same locally and in CI
- Easy to understand

### Pattern 2: Git hooks integration

```toml
# effigy.toml
[hooks]
pre-commit = ["effigy lint", "effigy test --fast"]
pre-push = ["effigy test"]

[hooks.install]
enabled = true  # Auto-install on first run
```

```bash
effigy hooks install  # Installs git hooks
effigy hooks run pre-commit  # Run manually
```

### Pattern 3: pre-commit compatibility

```yaml
# .pre-commit-config.yaml (optional)
repos:
  - repo: local
    hooks:
      - id: effigy
        name: Effigy
        entry: effigy ci --fast
        language: system
        pass_filenames: false
```

For teams already using pre-commit.

## 4) CI Provider Comparison

| Provider | Configuration | Free Tier | Self-hosted | Strengths |
|----------|--------------|-----------|-------------|-----------|
| GitHub Actions | YAML | Generous public | Yes | Integration, ecosystem |
| GitLab CI | YAML | Good | Yes | Integrated, simple |
| CircleCI | YAML | Limited | Yes | Performance, caching |
| Travis CI | YAML | Limited | No | Simple, historic |
| Buildkite | YAML | Paid | Required | Flexibility |

Effigy should work with all of them via the same interface.

## 5) Gaps and Opportunities

### Gaps in current tools

1. **Local CI testing**: Hard to test workflows locally
2. **Configuration drift**: CI and local configs diverge
3. **Multi-platform matrix**: Complex to set up
4. **Debugging**: Poor debugging experience

### Opportunities for Effigy

1. **Unified interface**: Same commands locally and in CI
2. **Local CI simulation**: `effigy ci --local` mimics CI
3. **Matrix testing**: `effigy test --matrix` across platforms
4. **Git hooks**: First-class hook management
5. **CI generation**: Generate CI configs from effigy.toml

## 6) Recommendations for Effigy

### Core Principle

> Effigy should be CI-agnostic. The same `effigy.toml` works locally, in CI, and on any platform.

### Specific Recommendations

**1. CI Entry Point**
```bash
effigy ci              # Full CI suite
effigy ci --fast       # Fast checks (pre-commit equivalent)
effigy ci --matrix     # Matrix across platforms (local simulation)
```

**2. Git Hooks**
```toml
# effigy.toml
[hooks]
pre-commit = ["effigy lint", "effigy test --fast"]
pre-push = ["effigy test"]
post-merge = ["effigy install"]
```

**3. CI Configuration Generation**
```bash
effigy ci init --provider github-actions  # Generates .github/workflows/ci.yml
effigy ci init --provider gitlab-ci       # Generates .gitlab-ci.yml
```

**4. Environment Consistency**
```toml
# effigy.toml
[ci.env]
CI = "true"  # Set automatically in CI mode
RUST_BACKTRACE = "1"

[ci.cache]
paths = ["target/", ".cargo/registry/"]
```

**5. Status Reporting**
```bash
effigy ci --format github  # Output for GitHub Actions annotations
effigy ci --format junit   # JUnit XML for CI reporting
```

## 7) Open Questions

- Should Effigy provide a GitHub Action wrapper?
- How to handle CI-specific secrets vs local env vars?
- What's the migration story for teams using existing CI configs?
- Should Effigy support pre-commit as a plugin/extension?

## 8) Next Steps

1. Prototype `effigy ci` command
2. Implement git hooks management
3. Create GitHub Actions example workflow
4. Design CI configuration generation
5. Research IDE integration (Track 13)
