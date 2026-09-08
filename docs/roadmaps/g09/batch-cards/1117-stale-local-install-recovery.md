# 1117 - Stale Local Install Recovery

Roadmap: [`../009-stale-local-install-recovery.md`](../009-stale-local-install-recovery.md)
Spec: [`../../../specs/124-stale-local-install-recovery-strict-lane.md`](../../../specs/124-stale-local-install-recovery-strict-lane.md)
Contract: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md)
Guide: [`../../../guides/057-bootstrap-repo-bringup.md`](../../../guides/057-bootstrap-repo-bringup.md)

Status: Ready
Owner: manifest error rendering and repository-local build provenance
Created: 2026-09-08
Queued by: operator-confirmed Chatterbox promotion and `northstar-queue`

## Purpose

Turn the self-hosting dead end into an exact, safe recovery message without
making manifest parsing permissive.

## Work

1. Reproduce with an Effigy `.local-install` binary whose recorded local SHA
   predates the checkout commit that added `[docs_policy.sources]`.
2. Add one reusable provenance check at the narrow build-info/error boundary:
   same checkout local-install path, resolvable local SHA, strict ancestor of
   current `HEAD`, and strict manifest parse failure.
3. Preserve the original parse error and add installed/current identities plus
   `cargo run --bin effigy -- bootstrap:local` as the recovery.
4. Add current-install, unprovable/divergent SHA, global/release binary, and
   consumer-repository controls. Assert text and JSON behavior.
5. Update guide `057`, close the named papercut, add `[Unreleased]` Fixed, and
   publish one evidence log.

## Acceptance

- [ ] reproduced stale self-install emits the exact source-build recovery with
      installed and current revisions
- [ ] current, unprovable, divergent, global/release, and consumer controls do
      not claim staleness
- [ ] original TOML error and non-zero result remain; JSON stdout parses and
      stderr/output ownership does not regress
- [ ] no schema fallback, automatic rebuild, network action, or manifest-key
      special case
- [ ] guide, papercut, changelog, and evidence agree
- [ ] coordinator closeout marks `g09` closed and removes live lane debris

## Review Oracle

Falsify spec `124` rows 1–6, especially a false stale diagnosis from an
unrelated local SHA or executable and a hint that depends on `doctor` parsing
the manifest first.

## Validation

- focused `effigy-core` build-info and runner manifest-error tests
- process-level stale/current/false-positive text and JSON proofs
- `effigy graph affected` for the implementation diff, then named targets
- `effigy qa`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`

## Evidence Requirement

One dated log under `docs/logs/2026-09/` names installed/current identities,
maps every oracle row, and records exact commands and results.

## Stop Conditions

Per spec `124`.

## Next Task

Open one implementation PR. After accepted review and merge, the queue
coordinator performs canonical `g09` closeout and returns to Chatterbox.
