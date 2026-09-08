# g09.009 Stale Local Install Recovery

Status: Ready
Created: 2026-09-08
Spec: [`124`](../../specs/124-stale-local-install-recovery-strict-lane.md)
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

- [ ] [`1117`](./batch-cards/1117-stale-local-install-recovery.md) — ready

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

## Non-Goals

- accepting unknown manifest keys, compatibility parsing, or schema downgrade
- automatically rebuilding or replacing the binary
- generic installed-version management for consumer repositories
- release, workflow, dependency, or manifest grammar changes

## Dispatch Manifest

- **Lane:** card `1117`, roadmap `g09.009`, strict spec `124`. State: ready.
- **Prerequisites:** card `1116` merged at `7d9c8be`; clean pushed `main`; no
  active Effigy queue task. **Completion:** implementation PR merged, evidence
  and docs accepted, then the coordinator closes `g09`, archives spec `124`,
  deletes the handoff, and leaves the runway empty for Northstar Refresh.
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

Execute card `1117`; then close `g09` and run Northstar Refresh.
