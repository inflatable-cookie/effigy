# 020 - Distribution Channel Proof And First-Publish Closeout

Generation: `g03`

Status: Planned
Owner: Platform
Created: 2026-05-02
Depends on: 019

## Problem

Effigy has distribution machinery and release automation, but the stable
channel story still depends too much on prior `v0.3.x` evidence and backlog
notes.

That is not enough for a clean `v1.0` posture. The first stable publish and
upgrade path need direct, current proof.

## Goal

Close the stable distribution lane on direct first-publish and upgrade
evidence across the supported install channels.

## Scope

- execute the first stable channel proof across:
  - tag install
  - Homebrew/tap install and upgrade
  - source install (`cargo install --git --tag`) as the fallback path
- validate the install and rollback surfaces against the `v1.0` release
  contract
- tighten the operator guidance for:
  - pinning
  - upgrade
  - rollback
  - CI installation
- produce one bounded closeout log that captures:
  - first-publish evidence
  - channel parity truth
  - any remaining operator caveats
- promote or retire the old backlog distribution notes so the active queue is
  the single source of truth

## Non-Goals

- adding new install channels just to widen reach
- wrapper-channel work
- reopening release automation unless first-publish evidence exposes a real
  gap

## Exit Condition

This milestone is complete when:

- stable install and upgrade paths are directly proven on the supported
  channels
- rollback and pinning guidance are validated against a real publish cycle
- the active roadmap queue, docs, and closeout evidence all agree on the
  stable distribution story

## Next Task

If this lane is promoted, run it as an evidence lane, not a docs-only lane.
The main deliverable should be a real first-publish closeout, not more
channel theory.
