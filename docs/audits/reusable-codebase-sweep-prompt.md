# Reusable Codebase Sweep Prompt

## Purpose

Use this prompt to run a deep, non-mutating audit of the Effigy codebase after a
release. The audit should identify structural improvement opportunities and
turn them into evidence-backed roadmap candidates.

This is not a lint pass. It is an architecture and maintainability sweep aimed
at making Effigy simpler, more modular, more obvious, and closer to
reference-grade systems programming.

## When To Use

Use this prompt:

- after a release, once release firefighting is complete
- before opening a new roadmap generation
- when the codebase feels harder to explain than it should
- when runtime flow, crate boundaries, or command orchestration have recently
  grown
- when you want a reusable audit report before deciding what to implement next

Do not use this prompt when the immediate goal is to fix a known bug, implement
an already-scoped roadmap, or run release commands.

## Reusable Prompt

````text
You are auditing the Effigy codebase after a release. Do not edit files. Do not
refactor. Do not open implementation batches. Produce an evidence-backed
architecture and maintainability audit report.

Goal:
Find structural improvement opportunities that would make the codebase simpler,
more modular, more obvious, and closer to reference-grade systems programming.

Audit for:
- duplication that should become shared modules
- split paths that should converge
- god files or oversized modules
- code clusters that could become clean crates
- crates too small or weakly justified and should merge
- tangled runtime logic
- awkward call stacks
- orchestration hidden in low-level modules
- data/model duplication across command surfaces
- leaky boundaries between CLI parsing, runner orchestration, domain crates,
  runtime execution, rendering, docs, and tests
- repeated report/JSON/text rendering patterns
- repeated fixture/test harness patterns
- places where behavior is difficult to explain, prove, or test
- stale abstractions left behind by earlier refactors
- naming that hides ownership or phase boundaries
- code that is correct but too clever, stateful, or indirect

Target standard:
Treat "reference grade" as:
- obvious ownership
- small stable interfaces
- low surprise control flow
- boring error handling
- no hidden global behavior
- domain logic in domain crates
- runner code as orchestration only
- public contracts documented
- tests proving behavior at the right layer
- no speculative abstractions
- no tiny crates without clear ownership
- no large files that require whole-system context to change safely

Required non-mutating exploration:

```sh
git status --short
find crates -maxdepth 2 -name Cargo.toml | sort
rg --files src crates tests docs | wc -l
effigy scan god-files --json
effigy scan duplicate-blocks --json
effigy scan comment-ratio --json
effigy scan attention-markers --json
effigy test --plan
````

Also inspect:
- Cargo.toml
- src/runner/**
- src/cli/**
- crates/*/src/**
- docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md
- recent docs/specs/ and docs/roadmaps/g04/ entries
- docs/contracts/
- current command/reference guides

If a scan command is unavailable or fails, record the failure in the report and
continue with manual inspection.

Important constraints:
- Do not edit code during the audit.
- Do not create roadmaps during the audit unless asked later.
- Do not run release commands.
- Do not modify .github/workflows/.
- Do not treat "more crates" as automatically better.
- Do not treat "fewer crates" as automatically better.
- Do not recommend abstractions without at least two concrete call sites or a
  clear ownership boundary.
- Do not propose rewrites.
- Prefer small staged cleanups with visible tests.

Produce the audit report using the output contract below.
```

## Expected Audit Report Shape

The audit report must start with:

```md
# Effigy Codebase Sweep Audit

Generated: <date>
Auditor: <agent/model/thread if useful>
Scope: <paths and surfaces inspected>
Release context: <release/version if known>

## Executive Summary

<short summary of the highest-leverage findings>

## Commands Run

| Command | Result | Notes |
| --- | --- | --- |
| `git status --short` | pass/fail | <summary> |
```

Then group findings under these categories:

1. `Duplication And Shared Abstractions`
2. `Split Paths And Convergence Opportunities`
3. `God Files And Oversized Modules`
4. `Crate Boundary Improvements`
5. `Crates To Merge Or Rejustify`
6. `Runtime And Execution Flow`
7. `CLI / Runner / Domain Separation`
8. `Error, Report, And JSON Contract Consistency`
9. `Test Harness And Fixture Simplification`
10. `Docs / Contracts / Implementation Drift`
11. `Reference-Grade Cleanups`

Each finding must include:

- title
- severity: `critical`, `high`, `medium`, `low`
- confidence: `high`, `medium`, `low`
- evidence: file paths, line refs, command output, or repeated symbols
- problem
- why it matters
- recommended direction
- what not to do
- likely files/modules affected
- suggested roadmap candidate, or `needs more investigation`

Finding template:

```md
### <Finding Title>

Severity: <critical|high|medium|low>  
Confidence: <high|medium|low>

Evidence:
- `<path>:<line>` — <what it shows>
- `<command>` — <relevant result>

Problem:
<short explanation>

Why It Matters:
<impact on maintainability, correctness, onboarding, release confidence, or
future feature work>

Recommended Direction:
<specific design direction>

What Not To Do:
<anti-pattern or overreach to avoid>

Likely Files / Modules:
- `<path>`

Suggested Roadmap Candidate:
<candidate block or `needs more investigation`>
```

## Severity And Priority Rules

Use these severity rules:

- `critical`: architecture makes future changes unsafe or blocks release
  confidence.
- `high`: clear structural debt with repeated cost or confusing ownership.
- `medium`: real simplification opportunity, but not urgent.
- `low`: polish, naming, docs drift, or opportunistic cleanup.

Do not inflate severity. A finding must be actionable and evidence-backed.

Prioritize by leverage, not by annoyance. Prefer findings that simplify future
work across multiple surfaces over local tidiness.

Use this roadmap candidate shape when a finding is actionable:

```md
### Candidate: <short title>

Goal:
<one sentence>

Scope:
- <specific work>
- <specific work>

Non-goals:
- <what not to change>

Acceptance criteria:
- <observable outcome>
- <test/docs expectation>

Suggested batch size:
<small | medium | large>

Suggested validation:
- <commands>
```

If a finding is too vague for a roadmap candidate, mark it as
`needs more investigation` and explain what evidence is missing.

The report must end with:

```md
## Priority Stack

1. <highest leverage candidate>
2. <next>
3. <next>

## Deferred / Not Worth Doing

- <item> because <reason>

## Suggested Next Planning Move

<one concrete next planning action>
```

## Recommended Evidence Sources

Use a mix of tool output and direct inspection.

Required command evidence:

```sh
git status --short
find crates -maxdepth 2 -name Cargo.toml | sort
rg --files src crates tests docs | wc -l
effigy scan god-files --json
effigy scan duplicate-blocks --json
effigy scan comment-ratio --json
effigy scan attention-markers --json
effigy test --plan
```

Recommended inspection targets:

- `Cargo.toml`
- `src/runner/**`
- `src/cli/**`
- `src/tests/**`
- `crates/*/src/**`
- `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`
- latest `docs/specs/0*.md`
- latest `docs/roadmaps/g04/*.md`
- `docs/contracts/`
- `docs/guides/025-command-reference-matrix.md`
- `docs/guides/026-json-payload-examples.md`
- `docs/guides/073-state-stack-guide.md`
- `docs/guides/074-deployment-guide.md`

Useful manual searches:

```sh
rg -n "TODO|FIXME|HACK|workaround|duplicate|legacy|temporary|shim|adapter" src crates docs
rg -n "serde_json::json!|println!|eprintln!|RunnerError|Command::new" src crates
rg -n "pub\\(|pub(crate)|pub mod|mod tests" src crates
rg -n "map_err|unwrap\\(|expect\\(" src crates
```

Treat generated output and historical roadmaps carefully. Historical docs can
explain why a boundary exists, but current code and active contracts are the
source of truth.

## Non-Goals

The audit must not:

- edit source files
- refactor code
- open implementation roadmaps or batch cards unless explicitly asked after the
  audit
- run release prepare or release execute
- modify `.github/workflows/`
- invent product scope
- recommend broad rewrites
- recommend crate splits without a stable ownership boundary
- recommend crate merges based only on line count
- collapse app-specific behavior into generic Effigy primitives

## Follow-Up Planning Flow

After the audit report is reviewed:

1. Select the top one to three roadmap candidates.
2. Promote durable behavior or architecture rules into `docs/contracts/` or
   `docs/architecture/` only when needed.
3. Create roadmap files or batch cards for selected work.
4. Keep each implementation batch bounded and independently verifiable.
5. Validate with targeted tests first, then broader QA once a coherent tranche
   lands.

The audit report is input to planning. It is not permission to start changing
code.
