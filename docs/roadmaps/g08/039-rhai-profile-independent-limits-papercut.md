# g08.039 Rhai Profile-Independent Limits Papercut

Status: Ready
Created: 2026-09-01
Spec: [`112`](../../specs/112-rhai-profile-independent-limits-strict-lane.md)
Card: [`1094`](./batch-cards/1094-fix-rhai-profile-dependent-expression-limits.md)
Guide: [`061`](../../guides/061-rhai-script-steps-guide.md)

## Purpose

Make one Rhai script parse the same way in debug and release Effigy builds
without weakening the finite parser limits or changing the public script API.

## Decision

- Effigy owns explicit Rhai expression-depth limits instead of inheriting
  `debug_assertions`-dependent defaults from the Rhai crate.
- Preserve the current release envelope: `64` at global scope and `32` inside
  script functions.
- Do not change call-stack, operation, collection-size, module, or host API
  limits in this lane.
- Add structural and adversarial tests so later dependency upgrades cannot
  silently restore profile-dependent parsing.

## Scope

- one shared configured-engine seam in `effigy-rhai`
- exact limit constants and profile-independent engine setup
- a function-body expression that exceeds the stock debug limit but remains
  inside Effigy's supported envelope
- proof that the finite upper bound remains enforced
- first-party script policy coverage where useful
- guide/changelog/evidence and papercut closeout

## Boundary

- no Rhai provider extraction or S3 movement
- no new manifest or CLI configuration
- no unlimited parser or runtime limits
- no call-stack or execution-budget changes
- no benchmark retrieval, graph-timeout, catalog-pack, release, or consumer
  migration work

## Cards

- [ ] [`1094`](./batch-cards/1094-fix-rhai-profile-dependent-expression-limits.md) — ready

## Acceptance

- debug and release hosts report the same explicit global/function expression
  limits: `64` / `32`
- a script function beyond Rhai's stock debug-only depth parses and runs through
  Effigy's configured engine
- a script beyond Effigy's explicit function limit still fails, proving the
  repair did not disable the guard
- current first-party scripts and the docs-context benchmark remain runnable
- focused and full repository validation pass
- closeout returns the queue to catalog-pack acquisition planning under
  contract `043`

## Next Task

Execute ready card
[`1094`](./batch-cards/1094-fix-rhai-profile-dependent-expression-limits.md).
