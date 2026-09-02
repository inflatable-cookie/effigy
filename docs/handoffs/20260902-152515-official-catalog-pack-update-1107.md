---
title: Official catalog-pack update 1107 worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Effigy official catalog-pack update
created: 2026-09-02
updated: 2026-09-02
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260902-152515-official-catalog-pack-update-1107.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, catalog-pack]
---

## What This Thread Was Doing

Cards `1103` through `1106` established the support floor, published the
official pack, and cut Effigy's compiled recovery baseline over to the exact
accepted artifact. Card `1107` is now ready: replace the placeholder official
coordinate and expose explicit `effigy service pack update` through the
existing acquisition transaction.

This dispatches one bounded implementation lane. No transcript or second
prompt is part of the authority chain.

## Why It Matters

The public update path is the user-facing payoff of the catalog ownership
split. It must remain explicit, digest-addressed, transactional, and no-op safe
without making ordinary Effigy use depend on GHCR.

## Current State

- **Repository:** `inflatable-cookie/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `6271b0ff129d006e47202b1b00def5ea7a395af8`
- **Pushed main verification:** the planning commit and this handoff must be on
  pushed `origin/main` before launch
- **Planning checkout:** clean before the readiness/handoff batch
- **Worker mode:** implementation worker dispatched by the orchestrator; this
  handoff activates worker-only worktree preflight
- **Planning artifacts included at the base:** card `1106` Complete; cards
  `1107` and `1108` Ready; spec `115`, roadmap `g08.048`, contract `043`, and
  front doors reconciled
- **Worker branch:** `worker/g08-048-official-catalog-pack-update-1107`
- **Worker worktree:** Paseo-managed worktree; launcher-selected path wins
- **Worktree creation command:** Paseo branch-off from pushed `origin/main`
- **Required sibling worktree links:** `effigy-catalog-pack`, source
  `/Users/tom/Dev/projects/effigy-catalog-pack`, destination beside the worker
  worktree as `../effigy-catalog-pack`
- **Active spec lane:** `docs/specs/115-catalog-pack-publication-and-cutover-strict-lane.md`
- **Roadmap milestone:** `docs/roadmaps/g08/048-catalog-pack-publication-and-cutover.md`
- **Ready card:** `docs/roadmaps/g08/batch-cards/1107-expose-official-catalog-pack-update.md`
- **Allowed runway:** card `1107` only
- **Remaining card budget:** one card
- **Dispatch topology:** parallel with card `1108`, which writes only the
  catalog-pack repository
- **Parallel safety check:** repository and mutable implementation surfaces are
  disjoint; shared milestone/spec/contract and front-door integration stays
  with the orchestrator
- **Surfaces this lane owns:** Effigy catalog channel/acquisition/runtime and
  CLI/help/rendering code; focused tests; guide `067`; changelog; card `1107`;
  one dated Effigy evidence log and its unique log-index entry
- **Integration ownership:** do not edit shared roadmap/spec/contract/front-door
  next-task prose beyond the card and unique evidence index; orchestrator
  integrates both parallel lanes after review/merge
- **Merge ordering:** same-repository PRs merge one at a time; orchestrator
  refreshes and re-reviews any changed head
- **Canonical refs:** architecture `026`; contract `043`; spec `115`; roadmap
  `g08.048`; card `1107`; accepted card `1106` evidence log
- **Review oracle:** card `1107` plus spec `115` whole-lane rows 6 and 7
- **Model capability profile:** ordinary bounded day-to-day implementation;
  use an economical non-frontier worker, with frontier review retained by the
  orchestrator
- **Frontier-worker justification:** none
- **Tool/runtime restrictions:** ordinary commands and QA remain network-silent.
  Live registry work is read-only except the user-invoked isolated update smoke.
  No pack publication, tag/channel movement, workflow edit, provider mutation,
  Effigy release, S3, retention, or extension-transport work.
- **Required validation:** every card `1107` Validation row; focused catalog,
  artifact, runner/CLI, and representative integration tests; isolated-home
  public `stable` update plus repeated no-op; `effigy qa`; fmt; clippy with
  warnings denied; `git diff --check`; repository doctor with no new errors
- **PR base/head:** current pushed `main` / worker branch above
- **PR URL:** pending
- **Review state:** awaiting implementation
- **Merge path:** orchestrator after accepted exact-head review and green checks

## Boundaries

- **In scope:** implement card `1107` completely through the existing artifact
  and acquire-validate-store-activate transaction.
- **Out of scope:** card `1108`, pack repository edits, publication/channel
  mutation, implicit update, new transport client, Effigy release, S3,
  retention, general extension transport, or command regrouping.
- **Outcome shape:** smallest complete contract-valid update surface,
  adversarial proof, evidence, and PR; not diagnostics-only.
- Preserve installed state exactly on resolution, pull, compatibility,
  validation, activation, and verified-no-op paths as required by the card.
- Do not invent architecture or widen the public command beyond `service pack
  update`. Stop on JSON breakage, artifact drift, non-public/invalid artifact,
  or a need to mutate `stable`.
- Work only in the clean worker worktree. Do not merge.

## Important Context

- **Planning lineage:** architecture `026` -> contract `043` -> spec `115` ->
  roadmap `g08.048` -> card `1107`.
- **Why ready:** card `1106` merged; public `v1.0.1` and `stable` resolve to
  `sha256:91de584e77487765c24f53abb63413783a99c0a7926c25aee1289a3cf370d9f3`;
  anonymous exact-byte pull and digest-bound attestation have accepted evidence.
- **Decisions:** official repository is fixed in Effigy-owned code; `stable` is
  resolved first but only its immutable digest enters acquisition; already-
  active verified content is a deterministic no-op; ordinary use never probes.
- **Open tensions:** reuse the existing artifacts adapter and transaction. Stop
  rather than introducing a second OCI client or hidden coordinate override.
- **Report after:** the command, atomicity/no-op oracles, live isolated smoke,
  and evidence are coherent, or at the first stop condition.
- **Report to:** the operator, who relays completion to the orchestrator.

## Suggested Next Move

Run worker preflight, read the card and canonical refs, then trace the existing
`service pack install` transaction and `pack::channel` placeholder. Design the
smallest digest-resolution seam and falsify failure/no-op state before widening
CLI documentation.

## Completion Protocol

Before broad reads, run `git rev-parse --show-toplevel`, `git branch
--show-current`, `git status --porcelain`, and `git worktree list --porcelain`.
Accept a clean launcher-provided non-main registered worktree. Otherwise stop
on a dirty/main launcher checkout; use the named manual fallback only under the
repository's worktree contract.

Fetch with bounded non-interactive SSH. Confirm selected `HEAD == origin/main`,
the planning base is its ancestor, and this handoff exists in selected `HEAD`;
load the tracked copy with `git show`. Verify the required sibling link before
work. Read `AGENTS.md`, the card, milestone, spec, architecture, contract, and
accepted `1106` evidence.

Complete one coherent implementation batch. Map every acceptance and review-
oracle counterexample to named tests/evidence. Update only the owned closeout
surfaces. If card `1108` changes shared prose, stop and leave integration to the
orchestrator.

Push the branch and open a PR against current pushed `main`. Report PR URL,
exact head, checks, evidence, unresolved items, and docs-QA classification. Do
not merge. Requested review changes return to this same worker branch.

