# 001 - Effigy Foundation

Status: Complete
Owner: Platform
Created: 2026-02-26
Depends on: none

## 1) Problem

Effigy has been extracted from Underlay, but needs first-class project structure for roadmap-driven development, docs, and release evolution.

## 2) Goals

- [x] Extract runner into standalone repository.
- [x] Migrate active consuming repos to `effigy` invocation.
- [x] Establish direct PATH-first installation guidance.
- [x] Define versioning and release workflow.
- [x] Add docs and reporting skeleton mirroring Underlay style.

## 3) Non-Goals

- [x] No breaking rewrite of catalog schema in phase 001.
- [x] No plugin system in phase 001.
- [x] No remote task registry in phase 001.

## 4) Execution Plan

### Phase 1.1 - Extraction and continuity
- [x] Port runner code from Underlay.
- [x] Preserve task semantics and built-in `repo-pulse` behavior.
- [x] Ensure tests pass in standalone crate.

### Phase 1.2 - Consumer migration
- [x] Migrate active repos from `underlay` runner invocation to `effigy`.
- [x] Rename catalogs to `effigy.toml` where owned.
- [x] Remove embedded runner crate from Underlay.

### Phase 1.3 - Documentation baseline
- [x] Create architecture docs scaffold.
- [x] Create roadmap index and phase-001 tracker.
- [x] Create reports conventions and templates.

### Phase 1.4 - Packaging and PATH workflow
- [x] Document PATH installation options (`cargo install --path`, local bin link).
- [x] Add release checklist for publishable binary artifacts.
- [x] Add smoke test matrix for PATH + wrapper invocation.

## 5) Acceptance Criteria

- [x] `cargo test` passes in `effigy`.
- [x] Migrated repos can run `effigy tasks` through existing wrapper scripts.
- [x] Underlay no longer contains the embedded runner crate.
- [x] PATH-based invocation is documented and validated.

## 6) Risks and Mitigations

- [x] Risk: cargo-run wrappers hide lock/contention issues.
  - Mitigation: document PATH-first execution and provide fallback wrapper guidance.
- [x] Risk: migration drift between repos.
  - Mitigation: keep a single report checklist with verification evidence.

## 7) Deliverables

- [x] Standalone `effigy` runner crate.
- [x] Migrated consuming repos (initial set).
- [x] Docs skeleton (`architecture`, `roadmap`, `reports`).
- [x] PATH + release runbook.
