# Make

Status: Complete
Tool name: GNU Make / BSD Make
Category: task runner (build system predecessor)
Owner:
Last updated: 2026-03-07
Scope: GNU Make 4.x documentation, POSIX make specifications, common usage patterns

## 1) Why this tool matters

Make is the baseline for task runners. Created in 1976, it remains ubiquitous in Unix-like development environments. Every developer has encountered it; many workflows are still built around it. Understanding Make's strengths and chronic problems explains why modern alternatives exist.

For Effigy, Make represents:
- The "good enough" default that new tools must displace
- A demonstration of how file-based dependency tracking works
- A cautionary tale about syntax and portability limitations

## 2) Product and era context

### Timeline

- **1976**: Created by Stuart Feldman at Bell Labs
- **1988**: GNU Make 1.0 released (Richard Stallman)
- **1992**: First POSIX Make standard (IEEE Std 1003.2)
- **2000s**: GNU Make becomes the de facto standard on Linux
- **2010s**: Modern alternatives begin emerging (Grunt, Gulp, npm scripts)
- **2020s**: Task, Just, and other alternatives gain traction

### Original Design Constraints

Make was designed for an era when:
- **Builds were simpler**: Single language (C), single target architecture
- **Disk I/O was expensive**: File timestamps were the cheapest change detector
- **Shell was universal**: Bourne Shell (sh) was the common interface
- **Memory was limited**: Keep state minimal, recompute on each run

### Evolution Patterns

GNU Make's evolution shows the tension between backward compatibility and modernization:

| Version | Year | Key Additions | Breaking Changes |
|---------|------|---------------|------------------|
| 3.0 | 1989 | VPath, conditionals | None |
| 3.81 | 2006 | `$(warning)`, `$(error)`, `eval` | None |
| 4.0 | 2013 | Job server for sub-Makes, loadable objects | None |
| 4.3 | 2020 | Grouped targets, `.FEATURES` | None |

The pattern: GNU Make adds features but cannot fix fundamental design limitations without breaking decades of existing Makefiles.

## 3) Defining architectural bets

### File-timestamp dependency tracking
- Core assumption: if file A is older than file B, A needs rebuilding
- Works well for C compilation, poorly for tasks without file outputs

### Shell-based execution
- Recipes are shell commands
- Leverages existing shell knowledge
- Tied to shell portability issues

### Implicit rules
- Built-in knowledge of common patterns (`.c` → `.o`)
- Reduces boilerplate but creates magic behavior

### Recursive Make
- Standard pattern: `$(MAKE) -C subdirectory`
- Creates coordination problems (parallelization, dependency ordering)

## 4) Standout strengths

- **Ubiquity**: Available on virtually every Unix-like system
- **Zero config for simple cases**: `target: dependencies` pattern is learnable in minutes
- **Incremental builds**: Only rebuilds what changed (when configured correctly)
- **Parallel execution**: `-j` flag for parallel jobs
- **Toolchain integration**: Every C compiler expects Make

## 5) Chronic weaknesses and recurring costs

### Syntax pain points

**Tab vs space sensitivity** — The most infamous Make problem. This Makefile:

```makefile
build:
    echo "building"
```

fails cryptically if spaces are used instead of tabs:
```
Makefile:2: *** missing separator.  Stop.
```

The error message doesn't say "use tabs not spaces" — it says "missing separator," confusing newcomers.

**No built-in help generation** — Most projects add boilerplate:

```makefile
.PHONY: help
help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-30s\033[0m %s\n", $$1, $$2}'
```

This is so common it has become copy-pasta, yet Make doesn't provide it natively.

**String manipulation arcana**:
- `$(subst from,to,text)` — simple substitution
- `$(patsubst pattern,replacement,text)` — pattern substitution with `%` wildcard
- `$(shell command)` — run shell command, capture output

These require reading the manual; discoverability is poor.

### Portability nightmares

**GNU vs BSD Make divergence** — Common incompatibilities:

| Feature | GNU Make | BSD Make |
|---------|----------|----------|
| Conditionals | `ifeq` | `.if` |
| Include directive | `include` | `.include` |
| Default shell | `/bin/sh` | `/bin/sh` but different flags |
| Functions | Extensive | Limited |

**Shell portability** — A Makefile using `echo -n` or `cp -r` will behave differently on macOS vs Linux.

**Windows** — Requires MSYS2, WSL, or Cygwin. Native Windows Make ports exist but are not standard.

### File-timestamp limitations

**The `.PHONY` workaround** — For tasks without outputs:

```makefile
.PHONY: test
test:
	cargo test
```

Without `.PHONY`, Make compares `test` file timestamp against dependencies. If a file named `test` exists, the recipe won't run.

**Clock skew** — In distributed builds (NFS, shared drives), timestamp comparisons fail when clocks differ.

**No content hashing** — Changing a file then changing it back still triggers rebuilds because mtime changed.

### Recursive Make considered harmful

Peter Miller's 1998 paper documents that recursive Make:
- Prevents proper parallelization (separate Make processes compete for `-j` slots)
- Breaks dependency ordering (parent doesn't know child's dependencies)
- Causes repeated work (same dependencies built in multiple subdirectories)

The "solution" (using a single top-level Makefile) scales poorly for large projects.

### Debugging difficulty

`-d` (debug) output is thousands of lines of internal state dumps. Common workaround:

```makefile
print-%: ## Print any variable
	@echo $* = $($*)
```

Then `make print-CFLAGS` shows the expanded value.

## 6) Between-release corrections

GNU Make evolution shows incremental improvement constrained by backward compatibility:

- **GNU Make 3.81 (2006)**: introduced `$(warning)` and `$(error)` for better debugging
- **GNU Make 4.0 (2013)**: loadable plugins (rarely used), job server for sub-Makes
- **GNU Make 4.3 (2020)**: grouped targets, `.FEATURES` variable for capability detection

The pattern: GNU Make adds features but can't fix fundamental limitations without breaking decades of existing Makefiles.

## 7) Effigy-relevant lessons

### Adopt carefully
- **Explicit task listing**: `make help` is a common convention; Effigy should have this built-in
- **Parallel execution**: Users expect `-j` equivalent; design for concurrent execution from start
- **Dependency tracking**: File-based is limited but proven; consider when to use it vs runtime checks

### Reject early
- **Tab-sensitive syntax**: Effigy should never have whitespace-sensitive meaning
- **Implicit rules/magic behavior**: Every task should be explicit and discoverable
- **Shell dependence by default**: Cross-platform should be the default, not an afterthought
- **Recursive execution without coordination**: If Effigy supports nested catalogs, they must coordinate properly

### Prototype before deciding
- Content-addressable caching vs timestamp-based
- How much file-based dependency tracking do users actually need vs want?

## 8) Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [GNU Make Manual](https://www.gnu.org/software/make/manual/) | official docs | 4.4 | high | Definitive reference |
| [GNU Make Release History](https://git.savannah.gnu.org/cgit/make.git/tree/NEWS) | changelog | 1988-2024 | high | Version evolution |
| [POSIX Make](https://pubs.opengroup.org/onlinepubs/9699919799/utilities/make.html) | standard | POSIX.1-2017 | high | Minimal common subset |
| [Recursive Make Considered Harmful](http://aegis.sourceforge.net/auug97.pdf) | paper | 1998 | high | Classic analysis by Peter Miller |
| [What's Wrong with GNU Make](https://gittup.org/tup/build_system_rules_and_algorithms.pdf) | analysis | 2009 | medium | Tup author's perspective |
| [BSD Make vs GNU Make](https://www.freebsd.org/doc/en/books/pmake/) | official docs | n/a | high | BSD Make documentation |
| [Makefile tutorial](https://makefiletutorial.com/) | tutorial | current | medium | Community resource |
| [Self-Documenting Makefile](https://marmelab.com/blog/2016/02/29/auto-documented-makefile.html) | blog | 2016 | medium | Common help pattern |

## 9) Open questions

- What percentage of Make's usage is actually for C compilation vs general task running?
- How do Windows developers handle Make in practice?
- What would Make users cite as the #1 reason they haven't switched?

## Next Task

Compare against Just and Task (taskfile.dev) in a Track 1 value-track synthesis on task configuration formats.

