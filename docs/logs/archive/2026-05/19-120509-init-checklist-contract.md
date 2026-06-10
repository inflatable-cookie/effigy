# Init Checklist Contract

Date: 2026-05-19  
Roadmap: [`g07.051`](../../../roadmaps/g07/051-init-context-inventory-and-checklist-contract.md)  
Batch card: [`1001`](../../../roadmaps/g07/batch-cards/1001-define-init-context-and-checklist-contract.md)  
Strict lane: [`093`](../../../specs/093-init-setup-wizard-strict-lane.md)

## What Changed

- pinned the shared init setup-job model around stable per-job fields:
  - `id`
  - `category`
  - `execution_kind`
  - `safety_class`
  - `applicability`
  - `default_selected`
  - `prerequisites`
  - `delegates_to`
  - `writes`
  - `summary`
- split setup work into three execution shapes:
  - `inspect`
  - `guidance`
  - `apply`
- pinned four safety classes:
  - `safe_check`
  - `safe_apply`
  - `contextual_apply`
  - `never_default`
- defined the new machine-facing payload:
  - command envelope: `effigy.command.v1`
  - result payload: `effigy.init.checklist.v1`
- pinned required checklist result fields and per-job fields so the TTY wizard,
  non-interactive action execution, and agent checklist flow all consume the
  same inventory
- classified the v1 job inventory across baseline, tasks, health, graph,
  secrets, runtime, bundles, docs, validation, and advanced setup surfaces
- made two deliberate boundaries explicit:
  - `guidance` jobs may recommend commands but cannot claim executable
    non-interactive action support
  - high-risk product surfaces such as deploy/state/distribution/release stay
    inspection-only from init v1

## Key Decisions

- the checklist contract is the source of truth; the wizard prompt flow must
  consume it later rather than inventing its own setup-step model
- job identifiers stay generic and product-shaped, not repo-specific
- package-script cleanup remains `guidance` in v1 because init does not yet own
  a bounded rewrite surface for that work
- `graph index` is treated as `safe_apply` because it writes only local graph
  state under `.effigy/`
- container bring-up and secrets-vault init remain contextual, not baseline

## Why This Matters

- `effigy init --checklist --json` now has a bounded target shape instead of a
  vague “list some actions” brief
- the next card can build TTY prompt phases against stable job metadata
- later non-interactive execution work can reuse the same inventory without
  re-deciding which jobs are runnable, default-selected, or only guidance

## Residual Limits

- this is planning-only; no CLI contract or implementation landed yet
- checklist v1 does not yet define a `--full-inventory` switch for including
  `not_applicable` jobs in every response
- explicit per-job ordering is still deferred to the wizard-engine card

## Next Task

Execute `1002`.
