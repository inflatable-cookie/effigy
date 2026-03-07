# Source Map 001: Task Runner Research

Status: Draft
Coverage: Make, Just, Task — baseline task runners
Last updated: 2026-03-07
Owner:

## Make (GNU Make, BSD Make)

### Tier 1: Primary Sources

| Source | URL | Notes |
|--------|-----|-------|
| GNU Make Manual | https://www.gnu.org/software/make/manual/ | Definitive reference |
| POSIX Make | https://pubs.opengroup.org/onlinepubs/9699919799/utilities/make.html | Minimal standard |
| GNU Make Source | https://git.savannah.gnu.org/cgit/make.git | Implementation |

### Tier 2: First-Party Technical Content

| Source | URL | Notes |
|--------|-----|-------|
| GNU Make Release Notes | In manual | Version changes |

### Tier 3: Practitioner Analysis

| Source | URL | Notes |
|--------|-----|-------|
| Recursive Make Considered Harmful | http://aegis.sourceforge.net/auug97.pdf | Classic paper |
| What's Wrong with GNU Make | https://gittup.org/tup/build_system_rules_and_algorithms.pdf | Comparative analysis |

## Just

### Tier 1: Primary Sources

| Source | URL | Notes |
|--------|-----|-------|
| Just Documentation | https://just.systems/man/en/ | Comprehensive |
| Just Repository | https://github.com/casey/just | Source, examples |
| Just Releases | GitHub releases | Changelog |

### Tier 2: First-Party Technical Content

| Source | URL | Notes |
|--------|-----|-------|
| README.md | https://github.com/casey/just/blob/master/README.md | Overview |

### Tier 3: Practitioner Analysis

| Source | URL | Notes |
|--------|-----|-------|
| GitHub Issues | https://github.com/casey/just/issues | Usage patterns |
| GitHub Discussions | https://github.com/casey/just/discussions | Q&A |

## Task (taskfile.dev)

### Tier 1: Primary Sources

| Source | URL | Notes |
|--------|-----|-------|
| Task Documentation | https://taskfile.dev | Official docs |
| Task Repository | https://github.com/go-task/task | Source |
| Task Releases | GitHub releases | Changelog |

### Tier 2: First-Party Technical Content

| Source | URL | Notes |
|--------|-----|-------|
| Task Cloud Blog Post | https://taskfile.dev/blog/introducing-task-cloud/ | Commercial direction |

### Tier 3: Practitioner Analysis

| Source | URL | Notes |
|--------|-----|-------|
| GitHub Issues | https://github.com/go-task/task/issues | Usage patterns |

## Format References

### Tier 1: Standards

| Source | URL | Notes |
|--------|-----|-------|
| TOML Specification | https://toml.io/en/v1.0.0 | Effigy's format |
| YAML Specification | https://yaml.org/spec/ | Task's format |
| POSIX Shell | https://pubs.opengroup.org/onlinepubs/9699919799/utilities/V3_chap02.html | Execution context |

## Cross-Cutting Analysis

### Comparative Resources

| Source | URL | Notes |
|--------|-----|-------|
| awesome-make | https://github.com/ianstormtaylor/awesome-make | Make resources |
| awesome-taskfiles | https://github.com/sh0rez/awesome-taskfile | Task examples |

## Research Gaps

Missing primary sources:
- [ ] BSD Make documentation (different from GNU)
- [ ] Microsoft nmake documentation
- [ ] Production migration stories (Make → Just/Task)

## Next Task

Expand to include build systems (Bazel, Buck2) for Track 02.

