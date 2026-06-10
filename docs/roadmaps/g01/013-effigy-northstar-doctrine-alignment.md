# 013 - Effigy Northstar Doctrine Alignment

Generation: `g01`

Status: Complete
Owner: Platform
Created: 2026-03-05
Depends on: 012

## Vision Alignment

This roadmap standardizes Effigy's docs system on the Northstar doctrine so
future roadmap and log work stays structurally consistent across projects.

## Primary Tags

- `MAINT`
- `OPERATE`

## Target Envelope

- Effigy uses generation-sharded roadmaps, month-sharded logs, and a clean
  canonical docs structure with no compatibility shim folders.

## Vision Target Delta

- Moved from a strong but project-specific docs layout to the shared Northstar
  structure used across the wider project set.

## 1) Problem

Effigy already had substantial vision and docs content, but its active docs
layout still used flat `roadmap/` and `reports/` sections that drifted from the
Northstar contract.

## 2) Goals

- [x] Migrate `docs/roadmap/` to `docs/roadmaps/g01/`.
- [x] Migrate `docs/reports/` to `docs/logs/YYYY-MM/`.
- [x] Move vision rollout history out of `docs/reports/` and into `docs/vision/history/`.
- [x] Update canonical readmes and path references to the new structure.

## 3) Non-Goals

- [x] No runtime or application code changes.
- [x] No broad rewrite of historical roadmap or log bodies beyond structural normalization.
- [x] No generation rollover to `g02` before it is actually needed.

## 4) Execution Plan

### Phase 13.1 - Roadmap sharding
- [x] Move numbered roadmap files into `docs/roadmaps/g01/`.
- [x] Keep backlog under `docs/roadmaps/backlog/`.
- [x] Add generation metadata and index files.

### Phase 13.2 - Log sharding
- [x] Move report artifacts into `docs/logs/YYYY-MM/`.
- [x] Add day-and-time filename prefixes while preserving sequential ordering per day.
- [x] Rewrite internal references to the new log paths.

### Phase 13.3 - Canonical docs cleanup
- [x] Update `docs/README.md`, `docs/roadmaps/README.md`, `docs/logs/README.md`, and root layout references.
- [x] Move vision rollout history to `docs/vision/history/`.
- [x] Remove obsolete folder references without leaving compatibility shims.

## 5) Acceptance Criteria

- [x] All active docs reference `docs/roadmaps/` and `docs/logs/`.
- [x] Roadmaps are segmented under `g01/` with the next milestone reserved as `014`.
- [x] Logs are segmented by month with day-time filename prefixes.
- [x] Vision rollout history is no longer stored under `docs/logs/`.

## 6) Risks and Mitigations

- [x] Risk: path rewrites leave stale references behind.
  - Mitigation: run targeted grep checks after the physical move.
- [x] Risk: historical artifact names lose logical ordering.
  - Mitigation: assign deterministic sequential time prefixes per day during migration.

## 7) Deliverables

- [x] `docs/roadmaps/g01/013-effigy-northstar-doctrine-alignment.md`
- [x] `docs/roadmaps/g01/README.md`
- [x] `docs/roadmaps/generation-index.md`
- [x] `docs/logs/archive/2026-03/05-201451-effigy-northstar-doctrine-alignment.md`

## 8) Validation

- [x] `rg -n 'docs/roadmap|docs/reports|g02/' docs README.md`
- [x] `find docs/roadmaps -maxdepth 3 -type f -name '*.md' | sort`
- [x] `find docs/logs -maxdepth 2 -type f -name '*.md' | sort`

## 9) Next Task

Open `g01.014` when Effigy receives the next net-new implementation milestone.
