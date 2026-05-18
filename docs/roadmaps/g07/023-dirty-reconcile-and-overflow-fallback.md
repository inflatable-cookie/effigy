# g07.023 - Dirty Reconcile And Overflow Fallback

Status: Complete
Depends on: `g07.022`

## Goal

Make watch mode safe when the backend delivers noisy, partial, or overflowed
event streams.

## Scope

- detect overflow or backend failure conditions
- mark watch state dirty when event fidelity is not trustworthy
- fall back to a reconcile pass built on incremental `graph index`
- prove that deletes and rename-like churn do not leave the graph stale

## Hard Boundaries

- no silent downgrade from overflow to "probably fine"
- no broad full DB reset unless the reconcile path proves insufficient
- no hidden background retries after the foreground watch process exits

## Acceptance

- overflow and backend-failure cases produce explicit watch events
- reconcile logic restores correct graph state after dirty fallback
- watch mode stays deterministic under repeated event bursts

## Next Task

Execute `964`.
