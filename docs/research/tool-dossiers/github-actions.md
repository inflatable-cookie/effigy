# GitHub Actions

Status: Draft
Tool name: GitHub Actions
Category: CI/CD platform (workflow automation)
Owner:
Last updated: 2026-03-07
Scope: GitHub Actions workflows, reusable actions, marketplace ecosystem

## 1) Why this tool matters

GitHub Actions is the dominant CI/CD platform. It's notable for:
- Integration with GitHub repositories
- YAML-based workflow configuration
- Reusable actions marketplace
- Matrix builds and complex workflows
- Free tier for public repositories

For Effigy, GitHub Actions represents:
- CI/CD integration patterns
- Workflow configuration conventions
- Marketplace/reusable component model
- Status reporting and checks

## 2) Product and era context

### Timeline

- **2018**: GitHub Actions announced
- **2019**: Public beta, marketplace launch
- **2020**: General availability
- **2021-2024**: Continuous feature expansion

### Design Philosophy

From GitHub documentation:

> "Automate your software development workflows"
> "Build, test, and deploy your code right from GitHub"
> "Community-powered workflows"

### Target Audience

- GitHub users (naturally)
- Open source projects
- Enterprises using GitHub
- Teams wanting integrated CI/CD

### Ecosystem

- **Actions Marketplace**: 20,000+ reusable actions
- **Community**: Widely adopted
- **Integrations**: Third-party tools
- **Self-hosted runners**: For private infrastructure

## 3) Defining architectural bets

### YAML workflow configuration

Workflows defined in `.github/workflows/`:

```yaml
name: CI
on: [push, pull_request]

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Build
        run: cargo build --release
      - name: Test
        run: cargo test
```

Benefits:
- Version controlled
- Familiar syntax
- Easy to read

### Event-driven execution

Workflows triggered by events:
```yaml
on:
  push:
    branches: [main]
  pull_request:
    branches: [main]
  schedule:
    - cron: '0 0 * * *'  # Daily
```

Event types: push, PR, release, schedule, manual, etc.

### Reusable actions

Actions encapsulate functionality:

```yaml
steps:
  - uses: actions/checkout@v4      # Official action
  - uses: actions/setup-node@v4    # Setup environment
  - uses: my-org/my-action@v1      # Custom action
```

Types of actions:
- JavaScript actions
- Composite actions (shell commands)
- Docker container actions

### Matrix builds

Test across configurations:
```yaml
strategy:
  matrix:
    os: [ubuntu, macos, windows]
    rust: [1.70, stable, nightly]
runs-on: ${{ matrix.os }}
steps:
  - uses: actions-rust-lang/setup-rust-toolchain@v1
    with:
      toolchain: ${{ matrix.rust }}
```

### Secrets management

Built-in secret handling:
```yaml
steps:
  - name: Deploy
    env:
      API_KEY: ${{ secrets.API_KEY }}
    run: ./deploy.sh
```

Organization-level and repository-level secrets.

## 4) Standout strengths

- **Integration**: Native GitHub integration
- **Free tier**: Generous for public repos
- **Marketplace**: Huge ecosystem of actions
- **Matrix builds**: Easy multi-platform testing
- **Self-hosted runners**: Private infrastructure option
- **Community**: Widely adopted, lots of examples

## 5) Chronic weaknesses and recurring costs

### YAML complexity

Complex workflows become verbose:
```yaml
# Can become hundreds of lines
# Debugging YAML is painful
# No types, limited validation
```

### Vendor lock-in

GitHub Actions is GitHub-specific:
- Can't easily migrate to GitLab CI or CircleCI
- Tied to GitHub ecosystem
- Pricing changes affect you

### Debugging difficulty

When workflows fail:
- Limited local testing options
- Have to push to test
- Debugging requires trial and error

### Performance

GitHub-hosted runners:
- Can be slow to start
- Shared resources
- Network egress limits

## 6) Between-release corrections

### Early Actions (2019-2020)
- Basic workflow support
- Limited actions

### Modern Actions (2021-2024)
- Reusable workflows
- Composite actions
- OIDC token support
- Larger runners
- Cache improvements

The pattern: Maturing from basic CI to comprehensive platform.

## 7) Effigy-relevant lessons

### Adopt carefully

- **YAML configuration**: Familiar, version controlled
- **Reusable components**: Action marketplace model
- **Event triggers**: Flexible execution
- **Matrix builds**: Multi-platform testing
- **Status reporting**: GitHub checks integration

### Reject early

- **Vendor-specific features**: Keep generic where possible
- **Complex YAML**: Don't replicate GitHub Actions complexity
- **Tight coupling**: Effigy should work with any CI

### Prototype before deciding

- Effigy GitHub Action
- Workflow examples
- CI integration patterns

## 8: Effigy CI Integration

### Option 1: GitHub Action

```yaml
# .github/workflows/ci.yml
- uses: effigy/effigy-action@v1
  with:
    tasks: 'test, lint, build'
```

Action wraps `effigy` command.

### Option 2: Simple workflow

```yaml
# .github/workflows/ci.yml
name: CI
on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install Effigy
        run: curl -sSL https://effigy.dev/install.sh | sh
      - name: Run tests
        run: effigy test
```

### Option 3: Matrix build

```yaml
strategy:
  matrix:
    os: [ubuntu, macos, windows]
runs-on: ${{ matrix.os }}
steps:
  - uses: actions/checkout@v4
  - name: Test on ${{ matrix.os }}
    run: effigy test
```

## 9: Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [GitHub Actions docs](https://docs.github.com/en/actions) | official docs | current | high | Primary reference |
| [Actions Marketplace](https://github.com/marketplace?type=actions) | marketplace | current | high | Ecosystem |
| GitHub blog/changelog | blog | ongoing | high | Updates |
| Community workflows | examples | ongoing | medium | Usage patterns |

## 10: Open questions

- How do teams manage workflow complexity?
- What's the migration story off GitHub Actions?
- How effective is the actions marketplace quality control?

## Next Task

Compare against pre-commit and other tools in Track 12 synthesis on CI integration.

