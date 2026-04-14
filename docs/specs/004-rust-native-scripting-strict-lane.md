# 004 Rust-Native Scripting Strict Lane

Status: paused
Updated: 2026-04-14
Roadmap: `g02.004`

## Context

The demo/browser lane is shipped and released. Cross-repo manifest cleanup is
also far enough along that the next real product-shaping question is scripting
policy: how Effigy should reduce shell sprawl in Rust-first repos without
forcing Bun into every Rust CI environment, while still leaving Bun + TS as the
default for web-oriented repos.

This spec wraps `g02.004` in strict planning grammar so the Rhai question gets
answered as a bounded product contract rather than dissolving into repo-by-repo
tool churn.

## Governing Refs

- `docs/architecture/product-guardrails.md`
- `docs/contracts/001-working-rules.md`
- `docs/roadmaps/generation-index.md`
- `docs/roadmaps/README.md`
- `docs/roadmaps/g02/README.md`
- `docs/roadmaps/g02/004-rust-native-scripting-surface-contract.md`

## Lane Focus

The active strict lane is:

- define the scripting policy split between Rust-first repos and web-oriented
  repos
- set the Rhai product boundary for Effigy-native scripting
- classify current non-Bun script surfaces into:
  - migrate early
  - migrate later
  - keep external for now
- make Jetstream's “full migration target” posture explicit instead of letting
  Python linger by inertia

This lane now has a shipped script-step foundation and a substantial Effigy
dogfooding surface across tasks, demos, long-running lifecycle, release
orchestration, and native distribution cutover. The first external pilot was
selected, then deferred for repo-boundary safety reasons rather than missing
capability. The lane is now paused on that clean internal boundary while the
distribution surface is split into its own optional cross-repo product lane.

## Current Posture

`strict-paused`

The Keepsake pilot is temporarily deferred. Effigy wrapper retirement and
native distribution cutover are now shipped. No further scripting batch is
active right now because the next product question is optional distribution
adoption, not missing Rhai capability.

## Batch Model

- planning stays in this spec plus the roadmap
- execution proceeds only from a ready card
- each ready card must leave the lane either:
  - with another explicit ready card
  - or back in planning with an intent checkpoint

## Intent Checkpoint

If the scripting question proves broader than one bounded batch, stop and ask
whether the priority is:

- Effigy product boundary
- repo migration policy
- or Jetstream-specific full-Rhai migration planning

Do not guess.

## Exit Condition

This strict lane is complete when Effigy has a bounded scripting strategy for
Rust-first repos, a concrete Rhai pilot slice, and an honest migration order
for the current mixed-runtime script surfaces.

## Next Task

Resume this lane only when an external Rhai pilot becomes safe again or a new
internal scripting capability gap appears that is not really a distribution
product question.
