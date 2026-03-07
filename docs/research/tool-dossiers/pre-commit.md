# pre-commit

Status: Draft
Tool name: pre-commit
Category: Git hook framework (code quality)
Owner:
Last updated: 2026-03-07
Scope: pre-commit hooks, code quality automation, git integration

## 1) Why this tool matters

pre-commit is a framework for managing git hooks. It's notable for:
- Multi-language hook support
- Configuration-driven setup
- Caching and dependency management
- Skip/override capabilities
- Wide adoption in Python and beyond

For Effigy, pre-commit represents:
- Local code quality automation
- Git hook integration patterns
- Configuration-driven tooling
- Developer workflow integration

## 2) Product and era context

### Timeline

- **2014**: Initial release (by Yelp)
- **2017**: v1.0, growing adoption
- **2020**: v2.0, multi-language support
- **2023**: v3.0, modern Python
- **Present**: Active maintenance

### Design Philosophy

From pre-commit documentation:

> "A framework for managing and maintaining multi-language pre-commit hooks"
> "Git hook scripts are useful for identifying simple issues before submission to code review"

### Target Audience

- Python developers (originally)
- Multi-language projects
- Teams wanting automated checks
- Open source projects

### Ecosystem

- **Hooks registry**: [pre-commit.com/hooks.html](https://pre-commit.com/hooks.html)
- **Maintained hooks**: Official and community hooks
- **Integration**: Works with any git repository
- **CI integration**: Can run in CI environments

## 3) Defining architectural bets

### Configuration-driven

Hooks defined in `.pre-commit-config.yaml`:

```yaml
repos:
  - repo: https://github.com/pre-commit/pre-commit-hooks
    rev: v4.5.0
    hooks:
      - id: trailing-whitespace
      - id: end-of-file-fixer
      - id: check-yaml

  - repo: https://github.com/psf/black
    rev: 23.12.0
    hooks:
      - id: black

  - repo: local
    hooks:
      - id: custom-lint
        name: Custom Linter
        entry: ./scripts/lint.sh
        language: system
```

Benefits:
- Version controlled
- Team-shared configuration
- Easy to update

### Multi-language support

Pre-commit supports hooks in:
- Python (conda, pip)
- Node.js (npm, yarn)
- Ruby (gem)
- Go
- Rust (cargo)
- Docker
- System executables
- And more...

Example Rust hook:
```yaml
- repo: local
  hooks:
    - id: cargo-check
      name: cargo check
      entry: cargo check
      language: system
      types: [rust]
```

### Caching and performance

Pre-commit manages:
- Virtual environments for each hook
- Repository clones
- Dependencies

First run is slow, subsequent runs are fast.

### Selective execution

Run only relevant hooks:
```bash
pre-commit run --all-files          # Run all hooks on all files
pre-commit run black                # Run specific hook
pre-commit run --files src/main.py  # Run on specific files
SKIP=lint pre-commit run            # Skip specific hooks
```

Files matched by `types`, `files` patterns.

### Stash management

Pre-commit automatically:
- Stashes unstaged changes
- Runs hooks on staged files only
- Restores stash after completion

This ensures hooks run on what will actually be committed.

## 4) Standout strengths

- **Multi-language**: Not limited to one ecosystem
- **Configuration-driven**: YAML config, version controlled
- **Caching**: Fast subsequent runs
- **Selective execution**: Run only what changed
- **Wide adoption**: Especially in Python community
- **CI integration**: Can run in CI for consistency
- **Local focus**: Catches issues before push

## 5) Chronic weaknesses and recurring costs

### Python dependency

Pre-commit is Python-based:
```bash
pip install pre-commit  # Requires Python
```

This is a barrier for non-Python projects.

### Hook startup overhead

Each hook has startup cost:
- Virtualenv activation
- Language runtime startup
- Tool initialization

Multiple hooks = multiple overheads.

### Configuration complexity

Complex projects can have verbose configs:
```yaml
# Can become hundreds of lines
# Multiple repos, versions to track
# Environment variables to manage
```

### Integration friction

With existing tooling:
- Duplication with CI checks
- Version drift between local and CI
- Conflicts with IDE formatters

## 6) Between-release corrections

### Early pre-commit (2014-2017)
- Python-only hooks
- Basic functionality

### Modern pre-commit (2018-2023)
- Multi-language support
- Performance improvements
- Better caching
- Rust, Go, Docker support

The pattern: Expanding beyond Python while keeping the same model.

## 7) Effigy-relevant lessons

### Adopt carefully

- **Configuration-driven**: YAML config is familiar
- **Multi-language support**: Don't limit to one ecosystem
- **Selective execution**: Run only what's needed
- **Stash management**: Handle unstaged changes properly
- **CI integration**: Local checks should work in CI too

### Reject early

- **Python dependency**: Effigy should be standalone
- **Multiple tool startups**: Consolidate where possible
- **Version drift**: Single source of truth

### Prototype before deciding

- Effigy git hook integration
- Pre-commit hook for Effigy
- Configuration patterns

## 8: Effigy Git Hook Integration

### Option 1: Effigy manages hooks

```toml
# effigy.toml
[hooks]
pre-commit = ["effigy lint", "effigy test --fast"]
pre-push = ["effigy test"]
post-checkout = ["effigy cache clean"]
```

Install:
```bash
effigy hooks install  # Creates .git/hooks/* scripts
```

### Option 2: Use pre-commit framework

```yaml
# .pre-commit-config.yaml
repos:
  - repo: local
    hooks:
      - id: effigy-lint
        name: Effigy Lint
        entry: effigy lint
        language: system
        pass_filenames: false

      - id: effigy-test-fast
        name: Effigy Fast Tests
        entry: effigy test --fast
        language: system
        pass_filenames: false
```

### Option 3: Hybrid approach

```toml
# effigy.toml
[hooks]
# Managed by pre-commit
pre-commit.external = "pre-commit run"

# Managed by Effigy
post-checkout = ["effigy cache clean"]
```

## 9: Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [pre-commit.com](https://pre-commit.com) | official docs | current | high | Primary reference |
| [pre-commit/hooks](https://pre-commit.com/hooks.html) | registry | current | high | Available hooks |
| GitHub pre-commit/pre-commit | source | v3.x | high | Implementation |
| Community configs | examples | ongoing | medium | Usage patterns |

## 10: Open questions

- How do teams handle pre-commit CI integration?
- What's the performance impact of many hooks?
- How to handle hook failures that need human review?

## Next Task

Compare against GitHub Actions and other tools in Track 12 synthesis on CI integration.

