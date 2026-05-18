# 912 - Close Code Graph Intelligence Proof

Roadmap: [`../012-performance-cache-and-regression-proof.md`](../012-performance-cache-and-regression-proof.md)
Strict lane: [`../../../specs/085-code-graph-intelligence-strict-lane.md`](../../../specs/085-code-graph-intelligence-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-17

## Purpose

Close the native code graph intelligence tranche with metrics, validation, and
accepted limitations.

## Scope

- benchmark indexing Effigy itself
- benchmark representative PHP and JS/TS fixtures
- record graph DB size
- record query latency
- record context-pack size and latency
- record stale/no-op index cost
- compare graph-assisted lookup against direct `rg` exploration
- update guides and agent skill guidance

## Guardrails

- no perfect-accuracy claim
- no "all languages supported" claim
- no release until limitations are explicit
- no hiding slow or low-value query paths

## Acceptance

- closeout log records metrics and limitations
- graph commands are predictable under fresh and stale state
- context packs are measurably smaller than broad file reads
- follow-up gaps are recorded as roadmap candidates

## Next Task

Open the next graph tranche only if the closeout limits justify more work now.
