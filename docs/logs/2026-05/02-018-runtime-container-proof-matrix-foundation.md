# 02-018 Runtime/Container Proof-Matrix Foundation

Date: 2026-05-02
Roadmap: `g03.018`
Batch: `358`

## What changed

- added a first explicit proof-matrix test surface for the hardened
  runtime/container core
- proved bootstrap runtime-session posture directly:
  - bootstrap setup work skips lease refresh
  - bootstrap start handoff forces stop-on-exit ownership
- proved reused-runtime activation parity directly:
  - standard routed tasks
  - deferred activation
  - explicit `exec`
  all keep gateway reconciliation in the same place, with lease refresh varying
  only when the typed session context says it should
- proved workspace cleanup parity directly:
  - public workspace
  - seeded task shell
  - bootstrap-forced public cleanup override

## Why it mattered

Before this batch, most of these seams had local tests, but the hardening lane
 still relied too much on reading several separate test files and inferring the
 matrix by hand.

This batch turns that into a smaller explicit proof surface.

## Result

The runtime/container core now has one first bounded proof matrix instead of
 just a pile of seam-level tests. The next lane decision can now ask whether
 one more proof slice is needed, rather than whether proof exists at all.
