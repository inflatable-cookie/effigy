# g09.009 Stale Local Install Recovery

Status: Complete (2026-09-08); PR `95` merged at `24e842196465813f960cea15cadd25d5857731fd`
Created: 2026-09-08
Spec: [`124`](../../specs/archive/124-stale-local-install-recovery-strict-lane.md)
Card: [`1117`](./batch-cards/1117-stale-local-install-recovery.md)
Guide: [`057`](../../guides/057-bootstrap-repo-bringup.md)
Origin: `PAPERCUTS.md` entry from 2026-09-05; reproduced twice by Chatterbox
and once by the operator; operator confirmed promotion and queue dispatch on
2026-09-08

## Purpose

Make Effigy's self-hosted local install fail helpfully when repository grammar
has moved ahead of the installed binary, including when the stale binary cannot
run the task that replaces itself.

## Problem

After `[docs_policy.sources]` landed, `.local-install/bin/effigy` rejected the
root manifest with `unknown field sources`. Every task failed before routing,
including `bootstrap:local`; the error gave no evidence that the installed
binary predated `main`. The only working recovery was the non-obvious source
route `cargo run --bin effigy -- bootstrap:local`.

## Decision

- Add a provenance-safe stale-local-install hint to the existing manifest parse
  error boundary.
- Diagnose only the repository's own `.local-install/bin/effigy` when its
  recorded local build commit is a strict ancestor of current `HEAD`.
- Keep strict parsing, error status, raw TOML detail, release behavior, and
  consumer behavior unchanged.
- Document the source-build recovery and close the papercut with process-level
  evidence.

## Cards

- [x] [`1117`](./batch-cards/1117-stale-local-install-recovery.md) — complete
      ([evidence](../../logs/2026-09/08-150412-stale-local-install-recovery-1117.md))

## Acceptance

- a provably stale repository-local install names installed/current revisions
  and the source-build refresh command on strict manifest failure
- current, unprovable, divergent, global/release, and consumer cases retain the
  ordinary parse error without a false stale claim
- text and JSON modes remain non-zero and preserve the original error; JSON
  stdout remains valid
- guide `057`, the papercut, and one evidence log agree on recovery
- post-merge closeout leaves `g09` closed with no ready card, active spec, or
  dispatch handoff

## Closeout Evidence

- PR `95` merged to `main` at `24e842196465813f960cea15cadd25d5857731fd`.
- Independent exact-head review was accepted at head
  `5be2a1248b5862bf6f69d43dbd1dc25e07953224`; the provider review records no
  blocking findings. Hosted checks and the focused validation listed in the
  evidence log passed.
- Spec `124` is archived, the `g09` front doors report no active strict lane,
  and this dispatch handoff is removed.
- Deferred non-blocking observations remain recorded in the evidence log:
  inaccurate test-count wording, a fail-closed transient git re-probe, and
  `doctor` not rendering the stale note.

## Non-Goals

- accepting unknown manifest keys, compatibility parsing, or schema downgrade
- automatically rebuilding or replacing the binary
- generic installed-version management for consumer repositories
- release, workflow, dependency, or manifest grammar changes

## Dispatch Manifest

- **Lane:** card `1117`, roadmap `g09.009`, strict spec `124`. State: complete;
  merged in PR `95` at `24e842196465813f960cea15cadd25d5857731fd`.
- **Prerequisites:** card `1116` merged at `7d9c8be`; clean pushed `main`; no
  active Effigy queue task. **Completion:** implementation PR merged, evidence
  and docs accepted. The coordinator closeout archived spec `124`, deleted the
  handoff, and left the runway empty for Northstar Refresh.
- **Owned mutable paths:** `crates/effigy-core/src/build_info.rs`,
  `src/runner/manifest.rs`, `src/runner/error.rs`, `src/runner/error/**`, focused
  tests under `src/tests/**` and the directly owned crate tests,
  `docs/guides/057-bootstrap-repo-bringup.md`, the named `PAPERCUTS.md` entry,
  `CHANGELOG.md` `[Unreleased]`, and one dated evidence log.
  **Reserved shared closeout surfaces:** this roadmap, card `1117`, spec `124`,
  `docs/specs/README.md`, `docs/specs/archive/README.md`, `docs/roadmaps/README.md`,
  `docs/roadmaps/g09/README.md`, `docs/roadmaps/generation-index.md`,
  `docs/logs/README.md`, `docs/contracts/001-working-rules.md`,
  `docs/vision/README.md`, and the dispatch handoff.
- **Concurrency:** no approved sibling lane; generation closeout is serial after
  the implementation merge.
- **Worker capability class:** general automatic pool; frontier justification:
  none. Required sibling worktree links: none.
- **Acceptance evidence and review oracle:** card `1117`, spec `124`, focused
  tests, process-level stale/current/false-positive proof, `effigy qa`, format,
  clippy, `git diff --check`, and one dated evidence log.
- **Stop conditions and escalation owner:** spec `124`; semantic or provenance
  ambiguity returns through the queue coordinator to Chatterbox.

## Next Task

`g09.009` and generation `g09` are closed. Return to Chatterbox for the
operator-requested Northstar Refresh; do not open `g10` from this closeout.
