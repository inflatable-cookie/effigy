# 021 - Root Manifest Dependency Pruning

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-05-03
Depends on: —

## Problem

The root `Cargo.toml` declares roughly 25 direct external dependencies, but at
least 15 have zero direct usage in `src/` and are not re-exported from the root
crate. Examples include `anstream`, `indicatif`, `reqwest`, `tokio`, `rhai`,
`semver`, `signal-hook`, `tabled`, `toml_edit`, `vt100`, `zeroize`, and others.

This creates unnecessary lock-file pressure, slower root-crate compiles, and a
maintenance surface that can drift from what the workspace members actually need.

## Goal

Prune the root `Cargo.toml` to only dependencies that `src/bin/` and `src/lib.rs`
directly touch.

## Scope

- audit every dependency in the root `Cargo.toml`
- remove dependencies with no direct `use` or `extern crate` in `src/`
- verify that workspace members still compile with their own declared deps
- run `cargo check` and `cargo test` after removal to confirm no breakage
- update any internal docs that reference the old root dependency list

## Non-Goals

- refactoring workspace member manifests
- upgrading dependency versions
- introducing new dependencies

## Exit Condition

This milestone is complete when the root `Cargo.toml` contains only dependencies
directly used by the root package, and the full workspace still compiles and
passes tests.

## Next Task

If this lane is promoted, start by running a dependency audit against `src/` and
`src/bin/` before making any edits.
