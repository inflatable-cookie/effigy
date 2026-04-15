# 009 - Vault-Backed Varlock Rollout

Generation: `g02`

Status: Planned
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

## Exit Condition

This milestone is complete when the secret-resolution contract is explicit and
the targeted repo cohort has moved off ad hoc local secret handling strongly
enough that vault-backed resolution is the default operating story.

## Next Task

Execute this rollout after the container framework, distribution release
closure, and demo/manifest-import rollout stop being the more immediate
operator priorities.
