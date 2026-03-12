# 015 - Effigy Self-Hosting and Agent-First Adoption

Generation: `g01`

Status: Complete
Owner: Platform
Created: 2026-03-06
Depends on: 005, 007, 011, 012, 013, 014

## Vision Alignment

This roadmap makes Effigy the default development surface in its own repo and
establishes the operator and agent contracts needed for broad project rollout.

## Primary Tags

- `OPERATE`
- `ROUTE`
- `MAINT`
- `RELEASE`

## Target Envelope

- Effigy can be installed locally as a dependable stable binary, invoked in
  dev mode through a dedicated wrapper, and used as the primary entrypoint for
  contributor loops, scripted QA, and AI-agent task execution.

## Vision Target Delta

- Moved from `Effigy supports project loops when explicitly chosen` toward
  `Effigy is the known default command surface for supported project work`.

## 1) Problem

Effigy already has the core mechanics for project orchestration:
- `effigy.toml` task catalogs,
- built-in `test`, `doctor`, `watch`, `init`, and `migrate`,
- PATH-first installation guidance,
- built-in Rust test selection that prefers `cargo nextest` when available.

But the product still does not fully embody its own intended operating model.
In practice:
- the Effigy repo does not yet dogfood a root `effigy.toml`,
- contributor guidance still leads with `cargo run`, `cargo test`, Cargo
  aliases, and direct shell scripts,
- local install guidance exists but does not yet define a stable user-level
  command path plus a first-class `effigy-dev` development entrypoint,
- script and helper surfaces are still mixed across shell wrappers, Cargo
  aliases, and repo-local binaries,
- AI agents lack one short, repo-portable contract that says how Effigy should
  be used before falling back to raw tool commands.

Until Effigy is self-hosting and opinionated in its own repo, operators and
agents will continue to treat it as optional glue instead of the primary
development loop.

## 2) Goals

- [x] Add first-party Effigy self-hosting in the Effigy repo through a root
  `effigy.toml`.
- [x] Define stable local command entrypoints for both installed and dev-mode
  invocation.
- [x] Make Effigy the canonical contributor surface for supported tasks in this
  repo.
- [x] Establish a single migration policy for arbitrary scripts and helper
  entrypoints.
- [x] Publish explicit agent-facing guidance for how to use Effigy in project
  contexts.
- [x] Make `effigy test` the known default for supported test flows, including
  `cargo nextest` preference when available.

## 3) Non-Goals

- [ ] No attempt to replace every external release/distribution wrapper in one
  batch.
- [ ] No breaking removal of compatibility scripts or Cargo aliases before
  their Effigy-first replacements are stable.
- [ ] No broad plugin/runtime extension system for agents in this roadmap.
- [ ] No forced cross-repo migration of every consumer project in the same
  milestone; this roadmap defines the baseline contract first.

## 4) Default Operating Contract

Effigy should define three local usage channels:

- stable operator channel: `effigy`
- source/dev channel: `effigy-dev`
- compatibility channels: existing Cargo aliases and thin wrapper scripts

Expected behavior:

- `effigy` resolves to a dependable locally built binary in a known path on the
  user's `PATH`.
- `effigy-dev` resolves to the current checkout through `cargo run --bin effigy --`.
- Effigy contributor docs should lead with `effigy ...` or `effigy-dev ...`
  rather than raw `cargo run` for routine use.
- Compatibility surfaces remain available during migration, but are documented
  as fallback or external-contract wrappers rather than the primary interface.

## 5) Script and Task Surface Policy

This roadmap should lock a migration policy instead of trying to settle every
implementation language debate up front.

Canonical policy:

- product logic, parsing rules, and behavioral validation belong in Rust code
  and Rust tests when practical,
- project-oriented orchestration belongs in `effigy.toml` tasks,
- shell scripts may remain as thin wrappers when they serve external workflow
  contracts, release/install boundaries, or low-level environment setup,
- task owners should prefer consolidating arbitrary scripts behind one Effigy
  task surface even when the implementation remains shell temporarily.

Decision gate to capture explicitly in docs:

- evaluate whether a single first-party script system is still needed after
  self-hosting tasks land and wrapper surfaces are reduced,
- if yes, choose it deliberately with criteria covering portability, testing,
  debuggability, and agent readability.

## 6) Agent-First Project Contract

Effigy should publish a compact default loop for AI agents working in repos that
adopt it:

1. discover available work with `effigy tasks`
2. inspect repo health with `effigy doctor`
3. inspect test routing with `effigy test --plan`
4. run supported work through `effigy <task>` or `effigy test ...`
5. fall back to raw tool commands only when Effigy does not yet cover the path

Agent docs should also define:

- how to tell when `tasks.test` overrides built-in detection,
- that `cargo nextest` is the preferred Rust runner when available,
- how to use `--json` for automation-safe output,
- how to interpret compatibility wrappers and when not to use them,
- minimum manifest coverage expected before a project claims Effigy adoption.

## 7) Execution Plan

### Batch 15.1 - Effigy Self-Hosting Baseline
- [x] Add a root `effigy.toml` to the Effigy repo.
- [x] Encode the main contributor loops as first-party tasks while keeping the
  built-in `test` command canonical:
  - `qa`
  - `qa:docs`
  - `qa:json`
  - `qa:json:ci`
  - `build:release`
  - `install:local`
  - `smoke:release`
- [x] Ensure Effigy can run its own repo workflows without relying on direct
  ad hoc commands in normal usage.
- [x] Preserve wrapper compatibility while routing canonical docs toward
  Effigy-first invocation.

### Batch 15.2 - Local Install and Entry Point Contract
- [x] Define the stable local install destination and wrapper strategy for
  `effigy`.
- [x] Add a first-class `effigy-dev` entrypoint that always runs the current
  checkout.
- [x] Document how local stable and dev channels coexist safely.
- [x] Add smoke coverage proving both channels work in normal operator flows.

### Batch 15.3 - Contributor Loop Migration
- [x] Rewrite core contributor docs to lead with `effigy` or `effigy-dev`.
- [x] Reduce first-party docs that recommend `cargo test` or raw shell scripts
  for supported paths.
- [x] Make `effigy test` and `effigy test --plan` the primary documented Rust
  test flow.
- [x] Keep raw Cargo/script guidance only where Effigy does not yet own the
  flow or where external contracts require it.

### Batch 15.4 - Script Surface Consolidation Policy
- [x] Inventory remaining non-Effigy helper entrypoints after self-hosting.
- [x] Classify each as:
  - migrate into product code/tests,
  - keep as thin external wrapper,
  - defer pending explicit script-system decision.
- [x] Publish the decision record for any retained multi-surface exceptions.

### Batch 15.5 - Agent and Cross-Repo Adoption Contract
- [x] Add an agent-facing guide for using Effigy in a project context.
- [x] Add reusable `AGENTS.md` snippet guidance for consumer repos.
- [x] Define minimum `effigy.toml` task coverage for declaring Effigy as the
  default loop in a repo.
- [x] Publish a rollout checklist for consumer-project adoption waves.

## 8) Acceptance Criteria

- [x] The Effigy repo has a root `effigy.toml` that covers the primary local
  contributor workflows.
- [x] A stable `effigy` command and a dev-mode `effigy-dev` command are both
  documented and validated.
- [x] Contributor docs consistently treat Effigy as the primary surface for
  supported tasks.
- [x] `effigy test` is the documented default Rust test path, with `nextest`
  preference and fallback behavior made explicit.
- [x] AI agents have a concise, explicit contract for how to use Effigy before
  raw tools.
- [x] Remaining wrappers and direct scripts are either justified as external
  contracts or queued for migration.

## 9) Risks and Mitigations

- [ ] Risk: stable and dev command channels drift or confuse operators.
  - Mitigation: keep channel purposes narrow, document them together, and add
    smoke checks for both.
- [ ] Risk: agent guidance drifts from real repo behavior.
  - Mitigation: make agent docs depend on self-hosted Effigy tasks and validate
    examples against the actual command surface.
- [ ] Risk: script consolidation expands scope before a clear policy exists.
  - Mitigation: lock classification rules first and defer any full script-system
    decision until post-inventory evidence is available.
- [ ] Risk: docs continue reinforcing Cargo/script-first habits.
  - Mitigation: update onboarding and common-command docs in the same roadmap,
    not as a later optional cleanup.

## 10) Deliverables

- [x] `docs/roadmaps/g01/015-effigy-self-hosting-and-agent-first-adoption.md`
- [x] root `effigy.toml` for the Effigy repo
- [x] stable/local install and `effigy-dev` entrypoint documentation
- [x] contributor doc updates for Effigy-first loops
- [x] helper-surface classification decision record
- [x] agent-facing usage guidance and rollout checklist

## 11) Validation

- [x] `rg -n "g01\\.015|015-" docs/roadmaps docs/README.md README.md`
- [x] roadmap indexes updated to point at `g01.015`
- [x] `./scripts/check-doc-links.sh README.md docs/roadmaps/README.md docs/roadmaps/g01/README.md docs/roadmaps/g01/015-effigy-self-hosting-and-agent-first-adoption.md`
- [x] `bash -n scripts/effigy-dev scripts/install-local-bin-links.sh`
- [x] `zsh -ic 'type -a effigy; type -a effigy-dev'`
- [x] `zsh -ic 'effigy-dev tasks'`
- [x] `zsh -ic 'effigy-dev test --plan'`
- [x] `zsh -ic 'effigy-dev bootstrap:local'`
- [x] `zsh -ic 'effigy tasks'`
- [x] `zsh -ic 'effigy-dev qa:docs'`
- [x] `zsh -ic 'effigy-dev --json tasks | jq -r .schema'`
- [x] `./scripts/check-doc-links.sh docs/logs/README.md docs/logs/2026-03/06-101500-remaining-helper-surface-classification.md docs/roadmaps/g01/015-effigy-self-hosting-and-agent-first-adoption.md`
- [x] `./scripts/check-doc-logs-index.sh`
- [x] `./scripts/check-doc-links.sh docs/README.md docs/guides/README.md docs/guides/047-agent-and-cross-repo-adoption.md docs/roadmaps/g01/015-effigy-self-hosting-and-agent-first-adoption.md docs/logs/README.md`
- [x] `git diff --check -- docs/README.md docs/guides/README.md docs/guides/047-agent-and-cross-repo-adoption.md docs/logs/README.md docs/logs/2026-03/06-103500-agent-and-cross-repo-adoption-contract.md docs/roadmaps/g01/015-effigy-self-hosting-and-agent-first-adoption.md`
- [ ] follow-on implementation batches must add command-level validation logs as
  they land

## 12) Next Task

ROADMAP COMPLETE.
