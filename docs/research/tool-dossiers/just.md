# Just

Status: Complete
Tool name: Just
Category: task runner (command runner)
Owner:
Last updated: 2026-03-07
Scope: Just 1.x documentation, GitHub repo, community usage patterns

## 1) Why this tool matters

Just is the most credible modern alternative to Make for general task running. Created by Casey Rodarmor (also creator of `ord` and contributor to Bitcoin), it explicitly focuses on being a command runner rather than a build system.

For Effigy, Just represents:
- A successful Rust-based task runner with traction
- A demonstration of modern syntax (not Makefile syntax)
- A narrowly-scoped tool that deliberately avoids certain features

## 2) Product and era context

### Timeline

- **2016**: Initial release by Casey Rodarmor
- **2018**: Gains traction in Rust community
- **2020**: Modules system added for larger projects
- **2021**: Significant function library expansion
- **2022-2024**: Regular releases with Windows improvements, completions

### Design Philosophy

From the README and documentation:

> "Just is a handy way to save and run project-specific commands"
> "Commands, called recipes, are stored in a file called `justfile`"
> "Just has a ton of useful features, and many improvements over Make"

### Explicit Scope Boundaries

Just deliberately avoids:
- File modification tracking (not a build system)
- Package management (use npm, cargo, etc.)
- Dependency resolution (use the package manager)

This narrow scope is a feature, not a limitation — it makes Just predictable.

### Community and Ecosystem

- **Primary language**: Rust projects (naturally), but multi-language adoption
- **crates.io**: ~25M+ downloads (as of 2024)
- **GitHub**: 20k+ stars
- **Packaging**: Available in most package managers (brew, cargo, apt, etc.)

## 3) Defining architectural bets

### Explicit command focus

No file modification tracking. Recipes run when explicitly invoked:

```justfile
build:
    cargo build

test: build
    cargo test
```

The `test` recipe depends on `build`, but this is explicit ordering, not file-based dependency checking.

### Modern, non-Make syntax

Justfile syntax example:

```justfile
# Default recipe (runs when just is invoked without arguments)
default:
    just --list

# Recipe with parameter
lint files="src":
    echo "Linting {{files}}"

# Recipe with multiple parameters
test filter="" retries="3":
    cargo test {{filter}} -- --retry {{retries}}

# Recipe with dependency
build: check-deps
    cargo build --release
```

Key syntax features:
- No tab sensitivity (any indentation works)
- `{{variable}}` interpolation (Go template style)
- Comments start with `#`
- Dependencies listed after colon

### Cross-platform shell abstraction

Just handles the platform differences:

```justfile
# Works on macOS, Linux, and Windows
hello:
    echo "Hello from {{os()}}!"
```

On Windows, Just uses `sh` by default (or PowerShell if configured), so standard Unix commands work.

#### Cross-platform strategy

Just's approach to cross-platform:

1. **Shebang handling**: Recipes with shebangs (`#!/bin/bash`) work on Windows via `sh` emulation
2. **PowerShell support**: Can use PowerShell on Windows with `set windows-powershell := true`
3. **Built-in functions**: `os()`, `arch()` for platform detection
4. **Path handling**: Automatic path separator conversion

Example with platform detection:
```justfile
build:
    {{ if os() == "windows" { "cargo build --target x86_64-pc-windows-msvc" } else { "cargo build" } }}
```

#### Windows compatibility

Just specifically addresses Windows:
- No MSYS/Cygwin required
- Works in cmd.exe, PowerShell, Windows Terminal
- Handles Windows path quirks

This is a key differentiator from Make, which struggles on Windows.

### Rich CLI experience

Built-in help (no boilerplate needed):

```bash
$ just --list
Available recipes:
    build    # Build the project
    test     # Run tests
    lint     # Run linter
```

Other CLI features:
- `--dry-run`: Show what would run without executing
- `--verbose`: Print recipes before running
- `--shell`: Override shell for a single run

## 4) Standout strengths

- **Better syntax than Make**: No tabs, clearer parameter syntax
- **Built-in help**: `just --list` shows all recipes with their comments
- **Dotenv integration**: Loads `.env` by default
- **Cross-platform**: Works on Windows without WSL
- **Rich CLI**: Colors, completions, better error messages
- **Fast**: Rust-based, starts quickly
- **Established**: Widely used in Rust ecosystem

## 5) Chronic weaknesses and recurring costs

### No dependency tracking
- Must run commands manually or chain them explicitly
- No "only run if source changed" capability

### Justfile syntax learning curve
- Yet another syntax to learn
- Not as familiar as shell or YAML
- Limited IDE/editor support compared to Make

### Limited workspace/monorepo support
- Can include other justfiles, but coordination is manual
- No built-in task discovery across a monorepo

### Shell quoting complexity
- Despite cross-platform goals, complex commands still hit shell differences
- Some users report Windows edge cases

### No caching
- Runs recipes every time invoked
- No input/output caching

## 6) Between-release corrections

Just has evolved steadily with regular releases:

- **Modules system**: Added to support larger justfiles
- **Function library**: Growing set of built-in functions
- **Better Windows support**: Ongoing improvements to PowerShell integration
- **Completions**: Expanded shell completion support

The pattern: Just stays focused on command running but adds conveniences within that scope.

## 7) Effigy-relevant lessons

### Adopt carefully
- **Explicit > implicit**: Just's explicit command model is clearer than Make's implicit rules
- **Built-in help**: `--list` with comments should be a baseline feature
- **Dotenv by default**: Loading `.env` is the right default
- **Cross-platform from day one**: Don't retrofit Windows support

### Reject early
- **Custom syntax**: Effigy should use standard formats (TOML), not invent new syntax
- **No caching**: A modern tool should have content-addressable caching
- **No monorepo awareness**: Task discovery across nested workspaces is essential

### Prototype before deciding
- Just's "modules" for larger projects — how well do they work?
- How much do users miss file-based dependency tracking?

## 8) Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [Just README](https://github.com/casey/just) | official docs | current | high | Primary source |
| [Just documentation](https://just.systems/man/en/) | official docs | current | high | Comprehensive reference |
| [Just changelog](https://github.com/casey/just/blob/master/CHANGELOG.md) | changelog | 2016-2024 | high | Version history |
| [Just on crates.io](https://crates.io/crates/just) | metrics | current | high | Download statistics |
| GitHub issues/discussions | community | ongoing | medium | Usage patterns |
| [Just modules documentation](https://just.systems/man/en/chapter_55.html) | official docs | current | high | Modules feature |
| [Casey Rodarmor blog/talks](https://rodarmor.com/) | blog | various | medium | Design philosophy |
| [Comparison with Make](https://just.systems/man/en/chapter_59.html) | official docs | current | high | Just's own comparison |

## 9) Open questions

- How many users use Just for simple personal projects vs team/company workflows?
- What features do users request most often that are declined (indicating scope boundaries)?
- How well does the modules system work for larger codebases?

## Next Task

Compare against Make and Task (taskfile.dev) in Track 1 synthesis.

