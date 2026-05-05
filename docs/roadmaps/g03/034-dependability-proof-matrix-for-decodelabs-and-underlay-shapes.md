# 034 - Dependability Proof Matrix For DecodeLabs And Underlay Shapes

Generation: `g03`

Status: Active
Owner: Platform
Created: 2026-05-05
Started: 2026-05-05
Depends on: [`033-runtime-container-caller-migration-and-cleanup.md`](./033-runtime-container-caller-migration-and-cleanup.md)

## Goal

Prove the new abstractions against representative DecodeLabs and Underlay
runtime/bootstrap shapes without mutating real project repos or production
data.

## Scope

- fixture repos for DecodeLabs bundle shape
- fixture repos for Underlay generated compose shape
- bootstrap target path proof
- inside-container re-entry proof
- host path and external mount proof
- manager operation report proof
- direct task, bootstrap task, and Rhai task parity proof
- DecodeLabs mysql seed proof where a Rhai script imports SQL via a
  container-targeted execution request, including `stdin_file` path handling

## Non-Goals

- fixing every app-specific bootstrap issue from the separate thread
- live production data access

## Next Task

Complete card
[`407-close-dependability-proof-matrix-for-decodelabs-and-underlay-shapes.md`](../../specs/batch-cards/407-close-dependability-proof-matrix-for-decodelabs-and-underlay-shapes.md).
