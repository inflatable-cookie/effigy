# 992 - Harden Large-Repo Scale And Storage

Roadmap: [`../043-large-repo-scale-and-storage-hardening.md`](../043-large-repo-scale-and-storage-hardening.md)
Strict lane: [`../../../specs/091-codegraph-parity-strict-lane.md`](../../../specs/091-codegraph-parity-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Make the graph surface predictable on repos larger than Effigy.

## Work

- build large-repo benchmarks without vendoring huge fixtures
- measure full index, incremental index, status, explore, DB size, and output
  size
- add schema migration tests for suite storage changes
- review SQLite settings and writer/reader behavior during watch refresh
- tighten generated/ignored/max-size behavior
- document rebuild and corruption recovery

## Acceptance

- large-repo benchmark log exists
- storage migration tests pass
- common warm-index queries remain cheaper than broad file exploration
- stale/failed path behavior is visible in reports

## Evidence

- [`2026-05/18-172629-large-repo-scale-and-storage-hardening.md`](../../../logs/archive/2026-05/18-172629-large-repo-scale-and-storage-hardening.md)

## Next Task

Execute `993`.
