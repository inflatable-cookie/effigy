# 009 - Vault-Backed Varlock Rollout

Generation: `g02`

Status: Complete
Owner: Platform
Created: 2026-04-15
Depends on: 025

## Problem

Effigy already ships the env-schema / varlock foundation, but the cross-repo
adoption path for moving secrets into 1Password or other vault-backed flows is
not yet standardized or rolled out.

## Goal

Turn the shipped varlock/env-schema capability into a clear rollout program for
consumer repos:

- provider posture
- local dev resolution story
- CI resolution story
- migration playbook
- consumer adoption order

## Scope

- define the rollout contract for vault-backed secret resolution
- prefer 1Password first without closing the door on other providers later
- roll the resulting posture across the intended consumer cohort

## Closeout

This rollout did not become a live `g02` execution lane.

The underlying varlock/env-schema product surface shipped, but the cross-repo
vault rollout was intentionally not run before `g02` closed.

## Exit Condition

This milestone is complete on the `g02` boundary because the generation is
closed and any future vault-backed adoption program must be re-sequenced
deliberately in the live queue.

## Next Task

Leave this roadmap closed.

If vault-backed rollout becomes active again, rehome it into the live
generation or backlog instead of pretending `g02` is still open.
