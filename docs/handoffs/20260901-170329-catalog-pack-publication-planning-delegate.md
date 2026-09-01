---
title: Catalog-pack publication and asset-cutover planning delegate handoff
kind: northstar-handoff
handoff_mode: planning-delegate
planning_mode: conversational-discovery
dispatch_authority: orchestrator
promotion_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Tom / Effigy orchestrator
created: 2026-09-01
updated: 2026-09-01
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260901-170329-catalog-pack-publication-planning-delegate.md
base_required: pushed-main
tags: [coordination, handoff, planning, conversation, catalog-pack, distribution, pr]
---

## What This Thread Was Doing

Effigy's in-repository catalog-pack acquisition prototype is complete. The
operator has now selected the official OCI repository and authorized the
scoped GitHub Actions changes needed to build, validate, and publish the pack.

This dispatches one operator-facing planning conversation for official pack
publication and concrete catalog-asset cutover. The delegate owns discovery
and evidence capture, not canonical promotion, implementation, publication, or
release execution.

## Why It Matters

The prototype proves safe acquisition and recovery, but Effigy still ships the
concrete catalog assets inside the core repository and the public official
channel does not exist. The next plan must establish independent asset
ownership without adding operator ceremony, weakening offline behavior, or
creating a dead `service pack update` command.

## Current State

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Base commit:** `0f40f7f2b1692628b078d76674f43fc2b4b79e46`
- **Pushed main verification:** local `HEAD` and `origin/main` both resolved to
  the base commit before this handoff was created.
- **Planning-delegate branch:** `planning/catalog-pack-publication`
- **Planning-delegate worktree:** Paseo-managed at launch; record the actual
  clean worktree path and branch rather than comparing them with placeholders.
- **Required sibling worktree links:** none.
- **Topic boundary:** official catalog-pack publication at
  `ghcr.io/inflatable-cookie/effigy-catalog-pack`, concrete catalog-asset
  ownership and source-of-truth cutover, supported automatic availability,
  stable-channel update semantics, recovery, and the scoped workflow changes
  needed to build, validate, and publish the pack.
- **Canonical context:** `AGENTS.md`; `README.md`; `docs/README.md`;
  `docs/architecture/026-feature-placement-and-command-surface.md`;
  `docs/contracts/001-working-rules.md`;
  `docs/contracts/043-feature-placement-and-surface-migration-contract.md`;
  `docs/roadmaps/README.md`; `docs/roadmaps/g08/README.md`;
  `docs/roadmaps/g08/040-catalog-pack-acquisition-prototype.md`;
  `docs/roadmaps/g08/batch-cards/1095-prototype-catalog-pack-acquisition.md`;
  `docs/logs/2026-09/01-092640-catalog-pack-acquisition-prototype-planning.md`;
  `docs/logs/2026-09/01-095641-catalog-pack-acquisition-prototype-1095.md`;
  `.github/workflows/`; `crates/effigy-catalog/catalog/`;
  `crates/effigy-catalog/src/pack/`.
- **Named triage packet:**
  `docs/triage/20260901-170329-catalog-pack-publication-and-cutover.md`.
- **Named research evidence:** none. Keep concise sourced publication research
  in the named triage packet.
- **Allowed write paths:** only the named triage packet.
- **Concurrent orchestrator work:** none. Open Effigy PR inventory was empty at
  dispatch preparation.
- **Frontier planning profile:** select the current frontier/high-reasoning
  conversational-planning profile from Paseo notes at launch.
- **PR base/head:** `main` to the launcher-provided planning branch.
- **PR URL:** pending.
- **Promotion owner:** orchestrator after accepted review and merge.

## Boundaries

- Keep the permanent compiled baseline and four-layer selection order settled
  by contract `043`; do not reopen prototype acquisition semantics.
- Design publication and cutover so existing service, container, system,
  workspace, and task workflows require no new mandatory command and ordinary
  commands perform no implicit registry probe.
- Distinguish the canonical source of concrete catalog assets from the compiled
  recovery baseline. Make regeneration, drift detection, and release ownership
  explicit rather than accepting two silently divergent copies.
- Define a compatible stable-channel model that resolves to immutable content,
  preserves the fixed baseline-owned repository, and lets public
  `effigy service pack update` succeed from its first release.
- Scope GitHub Actions changes to building, validating, and publishing the
  catalog pack. The operator has authorized those workflow edits for a later
  implementation lane; this planning delegate must not edit workflows.
- Separate workflow implementation from the irreversible first publication.
  The operator has not authorized this delegate to push an OCI artifact or run
  a release mutation.
- Talk directly with the operator. Separate operator-confirmed decisions,
  recommendations, sourced evidence, alternatives, and unresolved questions.
- Do not edit product code, workflows, architecture, contracts, specs,
  roadmaps, cards, logs, or front doors. Do not decide readiness or dispatch an
  implementation lane.
- Do not merge. The orchestrator owns review, intake, promotion, readiness,
  implementation routing, and later publication authority.

## Important Context

- **Known decisions:** official OCI repository is
  `ghcr.io/inflatable-cookie/effigy-catalog-pack`; the operator authorizes
  scoped `.github/workflows/` edits for pack build, validation, and publication;
  the compiled baseline remains permanent; project and user overrides retain
  precedence; normal commands never fetch; explicit OCI/local installation,
  atomic activation, fallback, rollback, and reset are already implemented;
  no automatic pruning is part of this lane unless separately decided.
- **Questions worth exploring:** where the canonical editable catalog assets
  live after cutover; whether pack versions are coupled to Effigy releases or
  independently versioned; how a stable channel maps to immutable digests;
  which supported install/init paths should make the official pack available
  automatically without harming offline/source installs; how the compiled
  baseline is generated and checked for drift; when `service pack update`
  appears; what authentication, permissions, provenance, and rollback evidence
  the publication workflow requires; and which irreversible first-publication
  step needs a later operator gate.
- **Research needs:** inspect the existing acquisition planner and artifact
  adapter, current release/distribution workflows, GHCR's official publishing
  and package-permission documentation, and current catalog asset layout. Use
  primary sources for external technical claims. Keep research bounded to
  choices that can change this plan.
- **Non-goals:** actual OCI publication; release prepare/execute; unrelated
  Effigy binary release changes; S3/Rhai provider extraction; general extension
  transport; command grouping; docs-context ranking/timeout work; automatic
  installed-pack garbage collection; or generation rollover.
- **Mainline drift risk:** none known at dispatch. Reconcile current `main`
  before opening the planning PR and record any relevant drift.
- **Stop conditions:** a need to publish or mutate release state; a proposed
  workflow scope beyond pack build/validation/publication; an unlisted write
  path; a change to settled acquisition semantics; topic expansion into S3,
  generic extensions, or Effigy release redesign; or an operator-owned choice
  that remains unresolved.

## Suggested Next Move

Read the named canonical context and start with two linked questions: where the
editable catalog source should live after cutover, and whether pack releases
should be independently versioned or tied to Effigy releases. Then work through
channel identity, automatic availability, workflow trust, update exposure, and
the first-publication gate in small groups.

At meaningful topic shifts, update the triage packet so the branch remains
useful without the private transcript.

## Completion Protocol

### Before the conversation

1. Confirm the current checkout is a clean, dedicated, non-`main` registered
   worktree. Start with `git rev-parse --show-toplevel`,
   `git branch --show-current`, `git status --porcelain`, and
   `git worktree list --porcelain`. Use the clean launcher worktree even if its
   generated branch differs from the planned branch; do not create another one
   for a name mismatch.
2. Run
   `GIT_SSH_COMMAND="ssh -o ConnectTimeout=10 -o BatchMode=yes" git fetch origin`.
   Confirm this handoff exists in `HEAD`, the recorded base is an ancestor, and
   the absolute file matches the tracked blob. The tracked handoff is
   canonical.
3. Required sibling links are `none`; skip link setup.
4. Read `AGENTS.md`, the named canonical refs, and any existing named triage
   packet. Do not treat the handoff or packet as execution authority.

### During the conversation

- Keep the operator in the loop directly; the orchestrator is not a message
  proxy and may continue unrelated work.
- Preserve exact operator decisions and label recommendations, evidence,
  alternatives, and open questions separately.
- Any research delegation is read-only and bounded. Reconcile its output before
  writing the packet.
- Stop on scope expansion, conflicting decisions, an unlisted write path,
  required implementation/publication, or a canonical change that cannot wait
  for promotion.

### When the planning packet is ready

1. Re-read the packet against the conversation. It must make asset ownership,
   version/channel identity, automatic availability, workflow trust and
   permissions, update exposure, migration/rollback proof, implementation
   sequencing, first-publication authority, recommendations, alternatives,
   unresolved questions, and proposed canonical destinations explicit.
2. Run `effigy qa:docs` and `git diff --check`. Inspect the complete branch
   diff; it may modify only the named triage packet.
3. Commit and push the launcher-provided planning branch, then open a PR against
   current `main`. The PR body lists the base/head, changed files,
   operator-confirmed decisions, recommendations, unresolved questions,
   research sources, validation, and the proposed promotion map.
4. Report the PR URL. Do not edit canonical surfaces or merge.

### Review, merge, and promotion

The orchestrator reviews the exact PR head for fidelity to this handoff and the
operator confirmations recorded in the packet, evidence quality, scope, and a
clean separation between confirmed decisions, recommendations, and open
questions. Requested changes return to this same delegate. Once accepted,
green, mergeable, and not paused, the orchestrator may merge without another
approval prompt.

Merge is intake, not promotion. The orchestrator then reconciles the packet
with current `main`, promotes settled meaning into canonical architecture,
contract, spec, roadmap, card, and front-door surfaces, and removes or splits
resolved triage material. Only that separate promotion may make an
implementation lane ready.
