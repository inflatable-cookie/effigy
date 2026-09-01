# 1100 - Preserve Ancestor Container Registry for Child Task Refs

Roadmap: [`../045-child-catalog-suite-registry-papercut.md`](../045-child-catalog-suite-registry-papercut.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md), [`../../../contracts/038-unified-test-orchestration-contract.md`](../../../contracts/038-unified-test-orchestration-contract.md)
Papercut: [`PAPERCUTS.md`](../../../../PAPERCUTS.md)

Status: Ready
Owner: test suite task-reference resolver
Created: 2026-09-01
Ready since: 2026-09-01 operator-approved papercut routing

## Purpose

Repair Effigy's resolver so a suite task reference can run at a child catalog
cwd without losing the originating repository's ancestor container registry.

## Work

- reproduce the parent `[containers]` plus child catalog task-ref failure in a
  synthetic Effigy fixture
- separate task cwd from loaded repository/catalog execution context
- preserve child explicit override precedence and normal direct-child discovery
- keep Acowtancy and its workaround untouched
- close this card with one evidence log

## Acceptance

- [ ] a child-catalog suite task ref with inherited `run_in = "container"`
      resolves the ancestor container default
- [ ] the expanded task retains the child catalog cwd and selector identity
- [ ] an explicit child registry/default wins over the ancestor fallback
- [ ] direct invocation from the child does not gain an undeclared ancestor
- [ ] command, Rhai, lifecycle, and same-catalog suite forms do not drift
- [ ] focused tests, changed-impact validation, `effigy qa`, Rust checks, and
      diff checks pass
- [ ] no Acowtancy file changes and no downstream workaround removal

## Review Oracle

Falsify these counterexamples before PR creation:

1. The recurrence only passes by changing the child task cwd back to the parent.
2. The ancestor registry is still absent when the task ref reaches execution.
3. Preserving the ancestor makes it override an explicit child registry.
4. Direct child invocation begins discovering ambient undeclared ancestors.
5. The fix special-cases Acowtancy/Cream names or changes manifest grammar.
6. Plan text/JSON claims a different cwd, task source, or container target than
   execution uses.

## Validation

- focused test-planning and execution recurrence tests
- `effigy graph affected` for changed source, then its direct test targets
- `effigy test --plan`
- `effigy qa`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`

## Evidence Requirement

Write one dated closeout log mapping every oracle row to exact proof. Record the
synthetic parent/child shape, resolved cwd and container target, unchanged
direct-child behavior, validation, and downstream ownership boundary.

## Stop Conditions

Stop if the repair requires Acowtancy edits, manifest grammar, ambient ancestor
discovery for direct commands, broad runner redesign, or removal of the
downstream workaround before revalidation.

## Next Task

Return the exact-head PR to the Effigy orchestrator. Acowtancy revalidation is
not part of this card.
