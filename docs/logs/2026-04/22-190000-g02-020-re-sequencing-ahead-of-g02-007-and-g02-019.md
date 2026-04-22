# g02.020 Re-sequencing Ahead Of g02.007 And g02.019

Date: 2026-04-22
Roadmap: `g02.020`
Affected lanes: `g02.007`, `g02.019`, `g02.020`

## Summary

`g02.020` (multi-project gateway expansion and service DNS) becomes the active
strict lane. `g02.007` (release prep) and `g02.019` (v0.3 surface audit) move
to queued posture behind it.

The prior ordering kept `020` staged behind a deliberate `v0.3.0` release cut
and a follow-up surface audit. That ordering assumed the release decision was
imminent. It is not; `007`'s `Next Task` has been parked on "stop in planning
until explicit release intent is provided" without operator action. Meanwhile
the product gap `020` closes — multi-project port collisions and absent TCP
service DNS — has surfaced as concrete friction in live consumer repos (see
`underlay-reference` pgweb integration where host-port hardcoding forced the
stack to collide with `contact-patch` and `compli-me`).

## Why Re-sequence

- `020`'s remaining work addresses a current, daily operator pain point, not a
  speculative scaling concern.
- `007`'s pause is discretionary — it waits on a human release decision, not
  on effigy work.
- `019` is a surface-audit lane that materially benefits from having the `020`
  DNS/port story settled first, since several of the audit surfaces describe
  gateway and port behavior.
- The route-model foundation (`301`) has already landed, so the opening move
  on `020` is already real in code.
- Taking `020` forward now keeps the staged continuation chain honest: `302`
  already chose loopback-IP allocation as the next concrete move, and batch
  card `303` is drafted and ready.

## What Changed

- `docs/specs/020-multi-project-gateway-expansion-and-service-dns-strict-lane.md`
  — `Status: staged` → `Status: active`; `Current Posture` note refreshed;
  `Next Task` repointed at executing `303`.
- `docs/specs/007-distribution-release-and-consumer-rollout-strict-lane.md` —
  `Status: active` → `Status: queued`; `Next Task` repointed to note `020` is
  now the active lane and release execution remains gated on explicit operator
  intent.
- `docs/roadmaps/g02/007-distribution-release-and-consumer-rollout.md` —
  `Status: In Progress` → `Status: Queued`; `Next Task` repointed.
- `docs/roadmaps/g02/019-v0-3-surface-audit-and-ux-simplification.md` —
  `Next Task` repointed at waiting for `020` to land before executing the
  post-audit alignment batch.
- `docs/roadmaps/g02/020-multi-project-gateway-expansion-and-service-dns.md` —
  `Status: Planned` → `Status: Active`; `Next Task` repointed at `303`.
- `docs/specs/batch-cards/303-implement-loopback-ip-allocation-and-gateway-setup-foundation.md`
  — `Status: staged` → `Status: next`; `Next Task` repointed at immediate
  execution.

No code changes. No batch card executes as part of this re-sequencing.

## Current State

- `g02.020` is now the active strict lane.
- `g02.007` remains paused at the same release-prep boundary. Release
  execution is still gated on explicit operator intent. No release state was
  altered.
- `g02.019` remains planned behind `g02.020`.
- Batch card `303` (loopback-IP allocation and gateway setup foundation) is
  the immediate next execution move.

## Boundary Call

`020` overtakes `007` and `019` in lane priority. `007`'s release decision is
not blocked by this — it remains available to resume whenever explicit release
intent lands. The re-sequencing is a priority swap, not a cancellation of
either queued lane.

Stale-doc risk is the main reason this note exists: future threads reading
`007`'s `Next Task` would otherwise be steered back to a parked release lane
instead of the active `020` work.

## Validation

- `cargo run --bin effigy -- qa:docs` (expected to pass; no code changes)
- `git diff --check`

## Next Task

Execute `303` — land loopback-IP allocation and gateway setup integration on
the bounded macOS path.

See
`docs/specs/batch-cards/303-implement-loopback-ip-allocation-and-gateway-setup-foundation.md`.
