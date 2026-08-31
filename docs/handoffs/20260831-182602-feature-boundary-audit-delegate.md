---
title: Effigy feature-boundary audit planning delegate handoff
kind: northstar-handoff
handoff_mode: planning-delegate
planning_mode: conversational-discovery
dispatch_authority: orchestrator
promotion_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Tom / Effigy orchestrator
created: 2026-08-31
updated: 2026-08-31
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260831-182602-feature-boundary-audit-delegate.md
base_required: pushed-main
tags: [coordination, handoff, planning, conversation, architecture, pr]
---

## What This Thread Was Doing

The operator raised a product-boundary concern while card `1089` was running:
Effigy's command and dependency surfaces have grown broad enough that current
feature placement deserves a fresh audit. The concrete trigger is the S3 host
API in `effigy-rhai`, which makes a provider-specific dependency and patched
`vendor/s3` source part of Effigy's release tree.

This dispatches one operator-facing planning conversation. Audit current
feature placement and develop a ranked boundary model. You own discovery and
evidence capture for this topic, not canonical promotion or implementation.

## Why It Matters

Effigy should remain one obvious orchestration entry point without becoming the
default implementation owner for every provider, runtime, and consumer concern.
A clear placement model can reduce operator clutter and dependency coupling
while preserving the parts of the broad façade that genuinely create routing,
safety, and automation value.

## Current State

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Base commit:** `dd65fd80fd9b62876efb010855dd2ab2ac930eb1`
- **Pushed main verification:** local `HEAD` and `origin/main` both resolved to
  the base commit before this handoff was created.
- **Planning-delegate branch:** `effigy-feature-boundary-audit`
- **Planning-delegate worktree:** Paseo-managed at launch; record the actual
  clean worktree path rather than comparing it with a placeholder.
- **Required sibling worktree links:** none.
- **Topic boundary:** Effigy's present command families, manifest domains,
  embedded runtimes, shipped catalogs, provider integrations, dependency
  coupling, and their correct product/ownership placement.
- **Canonical context:** `README.md`; `AGENTS.md`;
  `docs/architecture/000-overview.md`;
  `docs/architecture/010-package-map.md`;
  `docs/architecture/022-runtime-architecture-sanity-audit.md`;
  `docs/architecture/product-guardrails.md`;
  `docs/vision/020-strategic-runway-atlas-v1.md`;
  `docs/contracts/001-working-rules.md`; `Cargo.toml`; public help and built-in
  registries discovered through Effigy graph/source evidence.
- **Named triage packet:**
  `docs/triage/20260831-181909-command-surface-and-runtime-boundary-audit.md`.
- **Named research evidence:** none. Keep internal evidence and citations in
  the triage packet; propose a durable research destination at closeout if the
  material warrants one.
- **Allowed write paths:** only the named triage packet.
- **Concurrent orchestrator work:** implementation card `1089` runs in a
  separate worktree and may change docs/codegraph/CLI/help/JSON surfaces. Do
  not edit or promote those surfaces. Reconcile mainline drift before the PR.
- **Frontier planning profile:** frontier, high-reasoning conversational
  planning profile selected from current Paseo notes at launch.
- **PR base/head:** `main` to `effigy-feature-boundary-audit`.
- **PR URL:** pending.
- **Promotion owner:** orchestrator after accepted review and merge.

## Boundaries

- Inventory the whole feature surface at classification depth, then deep-dive
  only the highest-pressure seams. Start with S3/Rhai storage; compare other
  representative families such as state/deploy, containers/gateway,
  release/distribution, graph/scan/docs, and shipped service catalogs.
- Classify capabilities as Effigy core, reusable library/domain seam, optional
  runtime/provider, installed skill/extension, consumer-owned workflow, or
  deprecation/removal candidate.
- Judge placement by ownership clarity, command coherence, universality,
  routing/safety value, dependency and release coupling, provider specificity,
  and consumer evidence. Binary size is explicitly not a goal.
- Talk directly with the operator. Separate operator-confirmed decisions,
  recommendations, internal evidence, alternatives, and unresolved questions.
- You may use bounded read-only research subagents for inventory or evidence.
  They do not edit, create branches/PRs, contact the operator, or start workers.
- Do not edit product code, architecture, contracts, specs, roadmaps, cards,
  logs, front doors, Cargo files, or generated surfaces. Do not implement an
  extraction, select a roadmap lane, or make compatibility decisions silently.
- Do not merge. The orchestrator owns review, intake, promotion, and any later
  implementation runway.

## Important Context

- **Known decisions:** ownership clarity is primary; command-surface coherence
  is second; dependency-tree growth is a material concern; binary weight is
  not important. One `effigy` façade remains valuable, but current top-level
  families are not presumed permanent.
- **Questions worth exploring:** what makes a capability core even when it is
  not universal; whether extracted implementations should remain reachable
  through `effigy`; whether command families should group, move, or disappear;
  pre-`1.0` compatibility appetite; evidence thresholds for provider/runtime
  ownership; and which three to five seams deserve a follow-up decision.
- **Research needs:** build a source-backed feature/owner/dependency matrix;
  trace provider-specific dependencies to public surfaces; use recent consumer
  and planning evidence where available. External comparative research is
  optional and must remain bounded and cited.
- **Non-goals:** binary-size optimization, code refactoring, automatic crate
  splitting/merging, a plugin marketplace design, roadmap compilation, release
  work, or treating fewer commands as inherently better.
- **Mainline drift risk:** card `1089` may add `effigy docs context`; classify
  from the current public contract and flag its in-flight status rather than
  racing its files.
- **Stop conditions:** topic expansion beyond Effigy feature placement; a need
  to modify an unlisted path; conflicting operator decisions; implementation
  required to answer a planning question; or a proposed breaking choice whose
  owner is unclear.

## Suggested Next Move

Read this handoff, the named triage packet, and the canonical context. Start the
operator conversation with two edges: what deserves to count as Effigy core,
and how much pre-`1.0` command compatibility the recommendations should assume.
Then build the broad classification inventory and use S3/Rhai as the first
falsification case.

At meaningful topic shifts, update the packet so the branch remains useful
without the private transcript.

## Completion Protocol

### Before the conversation

1. Confirm the current checkout is a clean, dedicated, non-`main` registered
   worktree for the launcher-provided branch. Start with
   `git rev-parse --show-toplevel`, `git branch --show-current`,
   `git status --porcelain`, and `git worktree list --porcelain`. Use the clean
   launcher worktree even if its generated branch differs from the intended
   name; do not create another one for a name mismatch.
2. Run `GIT_SSH_COMMAND="ssh -o ConnectTimeout=10 -o BatchMode=yes" git fetch origin`.
   Confirm this handoff exists in `HEAD`, the recorded base is an ancestor, and
   the absolute file matches the tracked blob. The tracked handoff is canonical.
3. Required sibling links are `none`; skip link setup.
4. Read `AGENTS.md`, the named canonical refs, and the existing triage packet.
   Do not treat the handoff or packet as execution authority.

### During the conversation

- Keep the operator in the loop directly; the orchestrator may continue the
  unrelated card `1089` lane.
- Preserve exact operator decisions. Label recommendations, evidence,
  alternatives, and open questions separately.
- Keep any research delegation read-only and bounded. Reconcile its output
  before writing the packet.
- Stop on scope expansion, conflicting decisions, an unlisted write path,
  required implementation, or a canonical change that cannot wait for later
  orchestrator promotion.

### When the planning packet is ready

1. Re-read the packet against the conversation. It should contain a complete
   classification inventory, explicit criteria, pressure-point deep dives, a
   ranked recommendation set, alternatives, unresolved decisions, non-goals,
   and suggested canonical destinations.
2. Run `effigy qa:docs` and `git diff --check`. Inspect the full branch diff;
   it may modify only the named triage packet.
3. Commit and push the delegate branch, then open a PR against current `main`.
   The PR body lists base/head, the packet, operator-confirmed decisions,
   recommendations, unresolved questions, evidence, validation, and proposed
   promotion map.
4. Report the PR URL. Do not edit canonical surfaces or merge.

### Review, merge, and promotion

The orchestrator reviews the exact PR head for fidelity to this handoff and the
operator confirmations recorded in the packet, evidence quality, scope, and a
clean separation between decisions and recommendations. Requested changes go
back to this same delegate. Once accepted, green, mergeable, and not paused,
the orchestrator may merge without another approval prompt.

Merge is intake, not promotion. The orchestrator then reconciles the packet
with current `main`, resolves contradictory or still-operator-owned choices,
promotes settled meaning into canonical architecture/contracts/planning, and
removes or splits resolved triage material. Only then can an implementation
lane become ready.
