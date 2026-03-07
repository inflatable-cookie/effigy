# Rush

Status: Draft
Tool name: Rush
Category: monorepo tool (workspace management, change tracking)
Owner:
Last updated: 2026-03-07
Scope: Rush 5.x documentation, workspace configuration, change detection, build caching

## 1) Why this tool matters

Rush is Microsoft's monorepo tool for JavaScript/TypeScript. It's notable for:
- Deterministic builds with strict dependency management
- Change detection and incremental builds
- "Bulk suppression" for linting across packages
- Professional-grade monorepo workflows

For Effigy, Rush represents:
- Workspace dependency modeling
- Change detection patterns
- Professional monorepo workflows
- Strictness vs. flexibility tradeoffs

## 2) Product and era context

### Timeline

- **2016**: Rush created at Microsoft
- **2018**: Open-sourced
- **2020-2024**: Active development, ecosystem growth

### Design Philosophy

From Rush documentation:

> "A scalable monorepo manager for the web"
> "Professional monorepo workflows"
> "Deterministic and reproducible builds"

### Target Audience

- Large JavaScript/TypeScript monorepos
- Enterprises needing strict governance
- Teams wanting deterministic builds

### Positioning

Rush is more opinionated than npm/pnpm workspaces:
- Enforces dependency policies
- Requires explicit configuration
- Provides build orchestration
- Change detection for CI

## 3) Defining architectural bets

### Centralized configuration

Rush uses a central `rush.json`:

```json
{
  "$schema": "https://developer.microsoft.com/json-schemas/rush/v5/rush.schema.json",
  "rushVersion": "5.100.0",
  "pnpmVersion": "8.0.0",
  "nodeSupportedVersionRange": ">=18.0.0 <19.0.0",
  "projects": [
    {
      "packageName": "@myapp/core",
      "projectFolder": "apps/core"
    },
    {
      "packageName": "@myapp/shared",
      "projectFolder": "libs/shared"
    }
  ]
}
```

This centralizes workspace definition.

### Strict dependency policies

Rush enforces dependency rules:
```json
{
  "ensureConsistentVersions": true,
  "variants": [
    {
      "variantName": "production",
      "description": "Production dependencies"
    }
  ]
}
```

Policies:
- Consistent versions across packages
- Approved package lists
- No phantom dependencies

### Change detection

Rush tracks changes for incremental builds:

```bash
# Build only changed projects and dependents
rush build --to git:origin/main

# Build changed projects only (not dependents)
rush rebuild --from git:origin/main
```

Uses Git to detect file changes, then builds dependency graph.

### Build orchestration

Rush manages build order:
```bash
rush build  # Builds in dependency order
rush rebuild  # Force rebuild all
```

Understands:
- Inter-project dependencies
- Topological sort
- Parallel execution

### "Bulk suppression"

Rush can suppress linting errors across packages:
```json
{
  "bulkSuppressedRules": ["no-console"]
}
```

Useful for incremental adoption of strict rules.

## 4) Standout strengths

- **Determinism**: Strict policies ensure reproducibility
- **Change detection**: Build only what changed
- **Professional workflows**: Designed for large teams
- **Dependency validation**: Catches common mistakes
- **Build caching**: Incremental builds with tracking

## 5) Chronic weaknesses and recurring costs

### Configuration complexity

Rush requires significant setup:
- rush.json
- Common/config/rush/.npmrc
- Version policies
- Build cache configuration

Learning curve is steep compared to npm workspaces.

### JavaScript lock-in

Rush is JS/TS focused:
- Assumes package.json structure
- npm/pnpm/yarn integration
- Less applicable to other ecosystems

### Opinionated approach

Rush enforces its way:
- Strict dependency rules
- Centralized configuration
- May fight against for some workflows

## 6) Between-release corrections

### Rush 5.x evolution
- Improved change detection
- Better build caching
- Phased commands

### Recent improvements
- Better pnpm integration
- Improved performance
- Enhanced change detection

The pattern: Maturing toward enterprise-grade stability.

## 7) Effigy-relevant lessons

### Adopt carefully

- **Centralized catalog definition**: Clear workspace structure
- **Change detection**: Build/test only what changed
- **Dependency tracking**: Understand relationships
- **Strictness options**: Configurable validation

### Reject early

- **Centralized configuration only**: Effigy allows distributed catalogs
- **Language-specific assumptions**: Effigy is language-agnostic
- **Overly strict defaults**: Effigy should work out of box

### Prototype before deciding

- Change detection for effigy tasks
- Workspace dependency visualization
- Catalog relationship tracking

## 8) Comparison: Rush vs. Effigy

| Aspect | Rush | Effigy |
|--------|------|--------|
| Scope | JS/TS monorepos | Language-agnostic |
| Configuration | Centralized (rush.json) | Distributed (effigy.toml) |
| Change detection | Git-based | Could add |
| Dependency model | package.json deps | Task dependencies |
| Strictness | High | Configurable |

**Pattern**: Rush is comprehensive but opinionated. Effigy is flexible but could learn from Rush's change detection.

## 9) Effigy Enhancement Ideas

### Change detection

```bash
# Run tasks only in changed catalogs
effigy test --changed-from origin/main

# Run tasks in changed + dependents
effigy test --affected
```

### Workspace visualization

```bash
# Show catalog dependency graph
effigy catalogs --graph

# Output:
# api
# ├── core
# │   └── utils
# └── database
```

## 10) Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [Rush docs](https://rushjs.io/) | official docs | current | high | Primary reference |
| [Rush Stack](https://rushstack.io/) | official docs | current | high | Ecosystem |
| GitHub issues/discussions | community | ongoing | medium | Usage patterns |
| Microsoft blog posts | blog | various | medium | Design rationale |

## 11) Open questions

- How well does Rush scale to 1000+ packages?
- What's the overhead of strict dependency policies?
- How do teams handle Rush migration?

## Next Task

Compare against Nx and other tools in Track 08 synthesis on monorepo patterns.

