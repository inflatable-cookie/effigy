# 649 - Extract Shared Container Scope Fallback Helper

Roadmap: [`../025-container-command-decomposition.md`](../025-container-command-decomposition.md)
Strict lane: [`../../../specs/068-container-command-decomposition-strict-lane.md`](../../../specs/068-container-command-decomposition-strict-lane.md)
Contract: [`../../../contracts/023-container-command-decomposition-contract.md`](../../../contracts/023-container-command-decomposition-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-10
Updated: 2026-05-10

## Purpose

Remove the duplicated repo-root-versus-cwd fallback logic shared by container
status, down, and cache list.

## Scope

- add one shared helper that resolves:
  - repo-root scope when a real Effigy repo is available
  - invocation-cwd fallback when no repo applies
- move the manifest-root check into shared support ownership
- reuse the helper from lifecycle and cache command families
- keep all existing error text and fallback behavior unchanged

## Acceptance

- lifecycle and cache no longer duplicate the fallback matcher
- shared support owns the repo-root/cwd scope resolution seam
- focused container proofs stay green
