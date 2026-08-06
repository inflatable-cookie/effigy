# 1069 - Prove Patch Release Candidate

Roadmap: [`../026-patch-release-lane-hardening.md`](../026-patch-release-lane-hardening.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md),
[`release orchestration`](../../../guides/051-release-orchestration.md)

Status: Complete
Owner: Platform
Created: 2026-08-06
Ready after: card 1068

## Purpose

Run the complete read-only evidence needed to hand an authorized operator a
trustworthy `0.9.1` release candidate.

## Owner And Seam

This card owns validation, release status, docs closeout, and consumer-proof
claims. It does not own release mutation or downstream workaround removal.

## Work

- run focused release unit and CLI tests
- run formatting, Clippy, and configured release gates
- inspect `effigy release status`
- verify the lockfile synchronization behavior against Signal read-only before
  claiming its extra lockfile commit is obsolete
- record remaining downstream work for after `0.9.1`
- close card, roadmap, front doors, and one evidence log

## Acceptance

- [x] all configured release gates pass without bypass
- [x] release status has no unexplained blocker
- [x] Signal proof supports only claims actually demonstrated
- [x] no prepare, execute, tag, push, release, or downstream mutation occurs
- [x] lane state and next move are explicit

## Validation

- `cargo test -q -p effigy-release`
- `cargo test -q --test cli_output_tests release`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --all -- --check`
- `effigy release gates`
- `effigy release status`
- `git diff --check`

## Stop Conditions

Stop at any release mutation prompt, workflow edit, gate bypass, remote push,
or required downstream write.

## Next Task

Request explicit human authorization before any release prepare or execute
action.
